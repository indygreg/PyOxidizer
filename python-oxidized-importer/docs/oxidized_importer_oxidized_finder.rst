.. py:currentmodule:: oxidized_importer

.. _oxidized_finder:

===================================
``OxidizedFinder`` Meta Path Finder
===================================

:py:class:`OxidizedFinder` is a Python type implementing a custom and
fully-featured :ref:`meta path finder <oxidized_importer_meta_path_finders>`.
*Oxidized* is in its name because it is implemented in Rust.

Unlike traditional *meta path finders* which have to dynamically
discover resources (often by scanning the filesystem),
:py:class:`OxidizedFinder` instances maintain an *index* of known
resources. When a resource is requested, :py:class:`OxidizedFinder`
can retrieve that resource by effectively performing 1 or 2 lookups
in a Rust ``HashMap``. This makes resource resolution extremely efficient,
as no filesystem probing or other explicit I/O is performed.

Instances of :py:class:`OxidizedFinder` are optionally bound to binary
blobs holding *packed resources data*. This is a custom serialization format
for expressing Python modules (source and bytecode), Python extension
modules, resource files, shared libraries, etc. This data format
along with a Rust library for interacting with it are defined by the
`python-packed-resources <https://crates.io/crates/python-packed-resources>`_
crate.

When an :py:class:`OxidizedFinder` instance is created, the *packed resources
data* is parsed into a Rust data structure. On a modern machine, parsing
this resources data for the entirety of the Python standard library
takes ~1 ms.

:py:class:`OxidizedFinder` instances can index *built-in* extension modules
and *frozen* modules, which are compiled into the Python interpreter. This
allows :py:class:`OxidizedFinder` to subsume functionality normally provided by
the ``BuiltinImporter`` and ``FrozenImporter`` *meta path finders*,
allowing you to potentially replace ``sys.meta_path`` with a single
instance of :py:class:`OxidizedFinder`.

.. _oxidized_finder_in_pyoxidizer:

``OxidizedFinder`` in PyOxidizer Applications
=============================================

When running from an application built with PyOxidizer (or using the
``pyembed`` crate directly), an :py:class:`OxidizedFinder` instance will
(likely) be automatically registered as the first element in
``sys.meta_path`` when starting a Python interpreter.

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

The :py:class:`OxidizedFinder` instance will (likely) be associated with
resources data embedded in the binary.

This :py:class:`OxidizedFinder` instance is constructed very early during Python
interpreter initialization. It is registered on ``sys.meta_path`` before
the first ``import`` requesting a ``.py``/``.pyc`` is performed, allowing
it to service every ``import`` except those from the very few *built-in
extension modules* that are compiled into the interpreter and loaded as
part of Python initialization (e.g. the ``sys`` module).

If :py:class:`OxidizedFinder` is being installed on ``sys.meta_path``, its
:py:meth:`path_hook <OxidizedFinder.path_hook>` method will be registered
as the first item on ``sys.path_hooks``.

If filesystem importing is disabled, all entries of ``sys.meta_path`` and
``sys.path_hooks`` not related to :py:class:`OxidizedFinder` will be removed.

Python API
==========

See :py:class:`OxidizedFinder` for the Python API documentation.
