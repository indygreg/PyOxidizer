.. py:currentmodule:: starlark_pyoxidizer

.. _pyoxidizer_packaging_multiprocessing:

===========================================
Using the ``multiprocessing`` Python Module
===========================================

The :py:mod:`multiprocessing` Python module has special behavior and
interactions with PyOxidizer.

Worker Process Spawn Method
===========================

The :py:mod:`multiprocessing` module works by spawning work in
additional processes. It has multiple mechanisms for spawning
processes and the default mechanism can be specified by calling
:py:func:`multiprocessing.set_start_method`.

PyOxidizer has support for automatically calling
:py:func:`multiprocessing.set_start_method` when the :py:mod:`multiprocessing`
module is imported by :py:class:`oxidized_importer.OxidizedFinder`.
This behavior is configured via
:py:attr:`PythonInterpreterConfig.multiprocessing_start_method`.

The default value is ``auto``, which means that if the ``multiprocessing``
module is serviced by PyOxidizer's custom importer (as opposed to Python's
default filesystem importer), your application **does not** need to call
:py:func:`multiprocessing.set_start_method` early in its `__main__`
routine, as the Python documentation says to do.

To make the embedded Python interpreter behave as ``python`` would,
set :py:attr:`PythonInterpreterConfig.multiprocessing_start_method` to
``none`` in your configuration file. This will disable the automatic
calling of :py:func:`multiprocessing.set_start_method`.

If :py:func:`multiprocessing.set_start_method` is called twice, it
will raise ``RuntimeError("context has already been set")``. This
error can be suppressed by passing the undocumented ``force=True``
keyword argument to the function.

Buggy ``fork`` When Using Framework Python on macOS
---------------------------------------------------

The :py:mod:`multiprocessing` spawn methods of ``fork`` and ``forkserver``
are `known to be buggy <https://bugs.python.org/issue33725>`_ when Python
is built as a *framework*.

Python by default will use the ``spawn`` method because of this bug.

Since PyOxidizer does not use *framework* builds of Python, ``auto``
mode will use ``fork`` on macOS, since it is more efficient than
``spawn``.
