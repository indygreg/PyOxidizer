.. py:currentmodule:: oxidized_importer

.. _oxidized_finder_behavior_and_compliance:

==========================================
``OxidizedFinder`` Behavior and Compliance
==========================================

:py:class:`OxidizedFinder` strives to be as compliant as possible with
other *meta path importers*. So generally speaking, the behavior as
described by the
`importlib documentation <https://docs.python.org/3/library/importlib.html>`_
should be compatible. In other words, things should mostly *just work*
and any deviance from the ``importlib`` documentation constitutes a bug
worth `reporting <https://github.com/indygreg/PyOxidizer/issues>`_.

That being said, :py:class:`OxidizedFinder`'s approach to loading
resources is drastically different from more traditional means, notably
loading files from the filesystem. ``oxidized_finder`` breaks a lot of
assumptions about how things have worked in Python and there is some
behavior that may seem odd or in violation of documented behavior in Python.

The sections below attempt to call out known areas where
:py:class:`OxidizedFinder` deviates from typical behavior.

.. _no_file:

``__file__`` and ``__cached__`` Module Attributes
=================================================

Python modules typically have a ``__file__`` attribute holding a ``str``
defining the filesystem path the source module was imported from (usually
a path to a ``.py`` file). There is also the similar - but lesser known -
``__cached__`` attribute holding the filesystem path of the bytecode module
(usually the path to a ``.pyc`` file).

.. important::

   :py:class:`OxidizedFinder` will not set either attribute when
   importing modules from memory.

These attributes are not set because it isn't obvious what the values
should be! Typically, ``__file__`` is used by Python as an anchor point
to derive the path to some other file. However, when loading modules
from memory, the traditional filesystem hierarchy of Python modules
does not exist. In the opinion of PyOxidizer's maintainer, exposing
``__file__`` would be *lying* and this would cause more potential for
harm than good.

While we may make it possible to define ``__file__`` (and ``__cached__``)
on modules imported from memory someday, we do not yet support this.

:py:class:`OxidizedFinder` does, however, set ``__file__`` and
``__cached__`` on modules imported from the filesystem. So, a
workaround to restore these missing attributes is to avoid in-memory
loading.

.. note::

   Use of ``__file__`` is commonly encountered in code loading *resource
   files*. See :ref:`resource_files` for more on this topic, including
   how to port code to more modern Python APIs for loading resources.

.. _oxidized_finder_behavior_and_compliance_path:

``__path__`` Module Attribute
=============================

Python modules that are also packages must have a ``__path__`` attribute
containing an iterable of ``str``. The iterable can be empty.

If a module is imported from the filesystem,
:py:class:`OxidizedFinder` will set ``__path__`` to the parent
directory of the module's file, just like the standard filesystem
importer would.

If a module is imported from memory, ``__path__`` will be set to the
path of the current executable joined with the package name. e.g. if
the current executable is ``/usr/bin/myapp`` and the module/package name
is ``foo.bar``, ``__path__`` will be ``["/usr/bin/myapp/foo/bar"]``.
On Windows, paths might look like ``C:\dev\myapp.exe\foo\bar``.

Python's ``zipimport`` importer uses the same approach for modules
imported from zip files, so there is precedence for
:py:class:`OxidizedFinder` doing things this way.

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

:py:class:`OxidizedFinder` implements ``find_distributions()`` and
therefore provides the required hook for ``importlib.metadata`` to
resolve ``Distribution`` instances. However, the returned objects do
not implement the full ``Distribution`` interface.

Here are the known differences between ``OxidizedDistribution`` and
``importlib.metadata.Distribution`` instances:

* ``OxidizedDistribution`` is not an instance of
  ``importlib.metadata.Distribution``.
* ``locate_file()`` is not defined.
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

:py:class:`OxidizedFinder` implements ``iter_modules(prefix="")`` and
``pkgutil.iter_modules()`` should work. However, there are some
differences in behavior:

* ``iter_modules()`` is defined to be a generator but
  ``OxidizedFinder.iter_modules()`` returns a ``list``. ``list`` is
  iterable and this difference should hopefully be a harmless
  implementation detail.
* Support for the ``path`` argument to ``pkgutil.iter_modules()`` requires
  that :py:class:`OxidizedFinder`'s
  :meth:`path_hook <OxidizedFinder.path_hook>` is installed
  in ``sys.path_hooks``. This will be done automatically if
  :py:class:`OxidizedFinder` is installed at interpreter initialization time.

.. _oxidized_finder_path_hooks:

Paths Hooks Compatibility
=========================

The :py:meth:`OxidizedFinder.path_hook <OxidizedFinder.path_hook>` method
from an instantiated instance can be installed on ``sys.path_hooks`` to
enable a :py:class:`OxidizedFinder` to function as a
`path entry finder <https://docs.python.org/3/reference/import.html#path-entry-finders>`_.

As a brief refresher, callables on ``sys.path_hooks`` are called with
*paths*, giving them the opportunity to service a particular *path*.
If a *path hook* responds to a *path* by returning a *path entry finder*,
that returned object will service that *path*. Often, the *paths* passed
to *path hooks* are from ``sys.path``. However, arbitrary *paths* can be
passed in. A property of the returned *path entry finder* is it only
targets a particular level in the *package hierarchy*. Unlike *meta
path finders* (which can service any named resource it knows about),
*path entry finders* are *bound* to a specific package target level
and will only return resources existing at that level.

*path hooks* are used by the following mechanisms:

* The standard library `PathFinder <https://docs.python.org/3/library/importlib.html#importlib.machinery.PathFinder>`_
  (the meta path finder that Python uses to load resources from the
  filesystem) uses ``sys.path_hooks`` as part of resolving a *finder* for
  a given ``sys.path`` entry.
* ``pkgutil.get_importer()`` for resolving the finder for a given ``sys.path``
  entry. This in turn is used by various code, including other ``pkgutil``
  APIs.
* ``pkg_resources`` maps *path entry finder* types to functions to enable
  a resolution of ``pkg_resources.Distribution`` instances for individual
  *paths*.

When installed on ``sys.path_hooks``,
:py:meth:`OxidizedFinder.path_hook <OxidizedFinder.path_hook>` will respond
to the following path values:

* The path to the current executable, as defined by
  :py:attr:`OxidizedFinder.path_hook_base_str`.
* A virtual sub-directory of the path to the current executable, as defined by
  :py:attr:`OxidizedFinder.path_hook_base_str`.

.. important::

   :py:meth:`path_hook <OxidizedFinder.path_hook>` is very strict about
   what values it will respond to.

   The value **must** be a ``str`` and be equal to
   :py:attr:`OxidizedFinder.path_hook_base_str` or have
   :py:attr:`OxidizedFinder.path_hook_base_str` plus a directory separator
   as the exact string prefix.

   :py:meth:`path_hook <OxidizedFinder.path_hook>` will **not** respond
   to ``bytes``, ``pathlib.Path``, or any other path-like type.

   :py:attr:`OxidizedFinder.path_hook_base_str` **may not** be the same value as
   ``sys.executable``. Always use :py:attr:`OxidizedFinder.path_hook_base_str`
   to derive ``sys.path`` values to ensure the path hook will respond.

When :py:meth:`path_hook <OxidizedFinder.path_hook>` is called with its
:py:attr:`OxidizedFinder.path_hook_base_str` value, a
:py:class:`OxidizedPathEntryFinder` bound to the source
:py:class:`OxidizedFinder` is returned. This finder is able to service
*root resources* (i.e. top-level modules and packages).

When :py:meth:`path_hook <OxidizedFinder.path_hook>` is called with
a virtual sub-directory of :py:attr:`OxidizedFinder.path_hook_base_str`, the same
thing happens except the returned :py:class:`OxidizedPathEntryFinder`
will only service resources at the exact package hierarchy specified
by that virtual sub-directory.

The validation and normalization of path values is similar to the
following:

.. code-block:: python

   def path_hook(self, path: str):
       # Path exactly matching current_exe will be bound to resources at root.
       if path == self.path_hook_base_str:
           return ...

       # Virtual sub-directories must begin with self.current_exe + directory
       # separator.
       if not path.startswith((self.path_hook_base_str + "/", self.path_hook_base_str + "\\")):
           raise ImportError

       # Part after directory separator.
       package_part = path[len(self.path_hook_base_str) + 1:]

       # Normalize to UNIX style directory separators, allowing Windows
       # separators to exist.
       package_part = package_part.replace("\\", "/")

       # Ban leading, trailing, and consecutive directory separators.
       if package_part.startswith("/") or package_part.endswith("\\") or package_part.contains("//"):
           raise ImportError()

       # Ban dots in directory components.
       for part in package_part.split("/"):
           if part.startswith(".") or part.endswith(".") or part.contains(".."):
               raise ImportError()

       # Normalize directory tree to package hierarchy. e.g. foo/bar -> foo.bar.
       package = package_part.replace("/", ".")

       # When converting the package string to a Rust string to facilitate
       # resource name comparisons, it is encoded to UTF-8, replacing
       # "bad" code points with the Unicode replacement code point.
       rust_package_string = package.encode("utf-8", "replace")

Note that when the package component of virtual sub-directories is converted
to a Rust string, we use the UTF-8 encoding, not Python's active filesystem
encoding. This is to keep things simpler. And since :py:class:`OxidizedFinder`
indexes resource names using Rust's UTF-8 backed string type anyway, this seems
semantically correct from the perspective of ``oxidized_importer``.

As an example, if ``path`` were
``os.path.join(finder.path_hook_base_str, "a")``, the
finder would only service modules of the form ``a.*``. So ``a``, ``a.b`` would
match but ``a.b.c`` and ``d`` would not.

For best results, use ``os.path.join(finder.path_hook_base_str, str)`` to define
values that will be accepted by the path hook.

:py:class:`OxidizedPathEntryFinder` complies with the
`PathEntryFinder <https://docs.python.org/3/library/importlib.html#importlib.abc.PathEntryFinder>`_
protocol and implements :py:meth:`OxidizedPathEntryFinder.find_spec`
and :py:meth:`OxidizedPathEntryFinder.invalidate_caches`. However,
support for the deprecated methods ``find_loader`` and ``find_module``
is not implemented. Instances also implement
:py:meth:`OxidizedPathEntryFinder.iter_modules`, enabling it to be
used by ``pkgutil.iter_modules()``.

``pkg_resources`` Compatibility
===============================

:py:class:`OxidizedFinder` can be registered as a provider for
``pkg_resources``, enabling ``pkg_resources`` APIs to be used with
resources tracked by :py:class:`OxidizedFinder` instances.

However, there are known compatibility differences. See
:ref:`oxidized_finder_pkg_resources` for more.
