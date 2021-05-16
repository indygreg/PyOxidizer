.. py:currentmodule:: starlark_pyoxidizer

.. _pyoxidizer_packaging_multiprocessing:

===========================================
Using the ``multiprocessing`` Python Module
===========================================

The :py:mod:`multiprocessing` Python module has special behavior and
interactions with PyOxidizer.

In general, :py:mod:`multiprocessing` *just works* with PyOxidizer if
the default settings are used: you do not need to call any functions
in :py:mod:`multiprocessing` to enable :py:mod:`multiprocessing` to work
with your executable.

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
error can be suppressed by passing the ``force=True`` keyword
argument to the function.

Buggy ``fork`` When Using Framework Python on macOS
---------------------------------------------------

The :py:mod:`multiprocessing` spawn methods of ``fork`` and ``forkserver``
are `known to be buggy <https://bugs.python.org/issue33725>`_ when Python
is built as a *framework*.

Python by default will use the ``spawn`` method because of this bug.

Since PyOxidizer does not use *framework* builds of Python, ``auto``
mode will use ``fork`` on macOS, since it is more efficient than
``spawn``.

``spawn`` Only Works on Windows with PyOxidizer
-----------------------------------------------

The ``spawn`` start method is known to be buggy with PyOxidizer except
on Windows. It is recommended to only use ``fork`` or ``forkserver``
on non-Windows platforms.

.. important::

   If :py:class:`oxidized_importer.OxidizedFinder` doesn't service the
   :py:mod:`multiprocessing` import, the default start method on macOS
   will be ``spawn``, and this won't work correctly.

   In this scenario, your application code should call
   ``multiprocessing.set_start_method("fork", force=True)`` before
   :py:mod:`multiprocessing` functionality is used.

.. _pyoxidizer_packaging_multiprocessing_dispatch:

Automatic Detection and Dispatch of ``multiprocessing`` Processes
=================================================================

When the ``spawn`` start method is used, :py:mod:`multiprocessing` effectively
launches a new ``sys.executable`` process with arguments
``--multiprocessing-fork [key=value] ...``.

Executables built with PyOxidizer using the default settings recognize
when processes are invoked this way and will automatically call into
``multiprocessing.spawn.spawn_main()``, just as
:py:func:`multiprocessing.freeze_support` would.

When ``multiprocessing.spawn.spawn_main()`` is called automatically,
this replaces any other run-time settings for that process. i.e. your
custom code will not run in this process, as this is a *multiprocessing
process*.

This behavior means that :py:mod:`multiprocessing` should *just work* and
your application code doesn't need to call into the :py:mod:`multiprocessing`
module in order for :py:mod:`multiprocessing` to work.

If you want your code to be compatible with non-PyOxidizer running methods,
you should still call :py:func:`multiprocessing.freeze_support` early in
``__main__``, per the :py:mod:`multiprocessing` documentation. This function
should no-op unless the process is supposed to be a *multiprocessing
process*.

If you want to disable the automatic detection and dispatching into
``multiprocessing.spawn.spawn_method()``, set
:py:class:`PythonInterpreterConfig.multiprocessing_auto_dispatch` to ``False``.

Dependence on ``sys.frozen``
============================

:py:mod:`multiprocessing` changes its behavior based on whether
``sys.frozen`` is set.

In order for :py:mod:`multiprocessing` to *just work* with PyOxidizer,
``sys.frozen`` needs to be set to ``True`` (or some other truthy value).
This is the default behavior. However, this setting is configurable
via :py:attr:`PythonInterpreterConfig.sys_frozen` and via the Rust struct
that configures the Python interpreter, so ``sys.frozen`` may not always
be set, causing :py:mod:`multiprocessing` to not work.

Sensitivity to ``sys.executable``
=================================

When in ``spawn`` mode, :py:mod:`multiprocessing` will execute new
``sys.executable`` processes to create a worker process.

If ``sys.frozen == True``, the first argument to the new process will be
``--multiprocessing-fork``. Otherwise, the arguments are ``python``
arguments to define code to execute.

This means that ``sys.executable`` must be capable of responding to
process arguments to dispatch to :py:mod:`multiprocessing` upon process
start.

In the default configuration, ``sys.executable`` should be the PyOxidizer
built executable, ``sys.frozen == True``, and everything should *just work*.

However, if ``sys.executable`` isn't the PyOxidizer built executable,
this could cause :py:mod:`multiprocessing` to break.

If you want ``sys.executable`` to be an executable that is separate
from the one that :py:mod:`multiprocessing` invokes, call
:py:func:`multiprocessing.set_executable` from your application code to
explicitly install an executable that responds to :py:mod:`multiprocessing`'s
process arguments.

Debugging ``multiprocessing`` Problems
======================================

If you run into problems with :py:mod:`multiprocessing` in a PyOxidizer
application, here's what you should do.

1. Verify you are running a modern PyOxidizer. Only versions 0.17 and newer
   have :py:mod:`multiprocessing` support that *just works*.
2. Verify the *start method*. Call ``multiprocessing.get_start_method()``
   from your application / executable. On Windows, the value should be
   ``spawn``. On non-Windows, ``fork``. Other values are known to cause issues.
   See the documentation above.
3. Verify ``sys.frozen`` is set. If missing or set to a non-truthy value,
   :py:mod:`multiprocessing` may not work correctly.
4. When using ``spawn`` mode (default on Windows), verify
   ``multiprocessing.spawn.get_executable()`` returns an executable that
   exists and is capable of handling ``--multiprocessing-fork`` as its
   first argument. In most cases, the returned path should be the path of the
   PyOxidizer built executable and should also be the same value as
   ``sys.executable``.
