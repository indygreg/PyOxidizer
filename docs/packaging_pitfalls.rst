.. _pitfalls:

==================
Packaging Pitfalls
==================

While PyOxidizer is capable of building fully self-contained binaries
containing a Python application, many Python packages and applications make
assumptions that don't hold inside PyOxidizer. This section talks about
all the things that can go wrong when attempting to package a Python
application.

.. _pitfall_extension_modules:

C and Other Native Extension Modules
====================================

Many Python packages compile *extension modules* to native code. (Typically
C is used to implement extension modules.)

The way this typically works is some build system (often ``distutils`` via a
``setup.py`` script) produces a shared library file containing the extension.
On Linux and macOS, the file extension is typically ``.so``. On Windows, it
is ``.pyd``. When an ``import`` is requested, Python's importing mechanism
looks for these files in addition to normal ``.py`` and ``.pyc`` files. If
an extension module is found, Python will ``dlopen()`` the file and load the
shared library into the process. It will then call into an initialization
function exported by that shared library to obtain a Python module instance.

Python packaging has defined various conventions for distributing pre-compiled
extension modules in *wheels*. If you see an e.g.
``<package>-<version>-cp38-cp38-win_amd64.whl``,
``<package>-<version>-cp38-cp38-manylinux2014_x86_64.whl``, or
``<package>-<version>-cp38-cp38-macosx_10_9_x86_64.whl`` file, you are
installing a Python package with a pre-compiled extension module. Inside the
*wheel* is a shared library providing the extension module. And that shared
library is configured to work with a Python distribution (typically ``CPython``)
built in a specific way. e.g. with a ``libpythonXY`` shared library exporting
Python symbols.

PyOxidizer currently has :ref:`some support <status_extension_modules>` for
extension modules. The way this works depends on the platform and Python
distribution.

Dynamically Linked Python Distributions on Windows
--------------------------------------------------

When using a dynamically linked Python distribution on Windows (e.g.
via the ``flavor="standalone_dynamic"`` argument to
:ref:`config_default_python_distribution`, PyOxidizer:

* Supports importing shared library extension modules (e.g. ``.pyd`` files)
  from memory.
* Automatically detects and uses ``.pyd`` files from pre-built binary
  packages installed as part of packaging.
* Automatically detects and uses ``.pyd`` files produced during package
  building.

However, there are caveats to this support!

PyOxidizer doesn't currently support resolving additional library
dependencies from ``.pyd`` extension modules / shared libraries when
importing from memory. If an extension module depends on another shared
library (almost certainly a ``.dll``) outside the normal set of libraries
(namely the C Runtime and other common Windows system DLLs), you will
need to manually package this library next to the application ``.exe``.
Failure to do this could result in a failure at ``import`` time.

PyOxidizer does support loading shared library extension modules from
``.pyd`` files on the filesystem like a typical Python program. So
if you cannot make in-memory extension module importing work, you
can fall back to packaging a ``.pyd`` file in a directory registered
on ``sys.path``, as set through the :ref:`config_python_interpreter_config`
Starlark primitive.

Extension Modules Everywhere Else
---------------------------------

If PyOxidizer is not able to easily reuse a Python extension module
built or distributed in a traditional manner, it will attempt to
compile the extension module from source in a way that is compatible
with the PyOxidizer distribution and application configuration.

The way PyOxidizer achieves this is a bit crude, but effective.

When PyOxidizer invokes ``pip`` or ``setup.py`` to build a package, it
installs a modified version of ``distutils`` into the invoked Python's
``sys.path``. This modified ``distutils`` changes the behavior of some
key build steps (notably how C extensions are built) such that the build
emits artifacts that PyOxidizer can use to integrate the extension module
into a custom binary. For example, on Linux, PyOxidizer copies the
intermediate object files produced by the build and links them into the
same binary containing Python: PyOxidizer completely ignores the shared
library that is or would typically be produced.

If ``setup.py`` scripts are following the traditional pattern of using
`distutils.core.Extension <https://docs.python.org/3/distutils/apiref.html#distutils.core.Extension>`_
to define extension modules, things tend to *just work* (assuming extension
modules are supported by PyOxidizer for the target platform). However,
if ``setup.py`` scripts are doing their own monkeypatching of
``distutils``, rely on custom build steps or types to compile extension
modules, or invoke separate Python processes to interact with ``distutils``,
things may break.

If you run into an extension module packaging problem that isn't
recorded here or on the :ref:`static page <status_extension_modules>`,
please `file an issue <https://github.com/indygreg/PyOxidizer/issues>`_ so
it may be tracked.

Identifying PyOxidizer
======================

Python code may want to know whether it is running in the context of
PyOxidizer.

At packaging time, ``pip`` and ``setup.py`` invocations made by PyOxidizer
should set a ``PYOXIDIZER=1`` environment variable. ``setup.py`` scripts,
etc can look for this environment variable to determine if they are being
packaged by PyOxidizer.

At run-time, PyOxidizer will always set a ``sys.oxidized`` attribute with
value ``True``. So, Python code can test whether it is running in PyOxidizer
like so::

   import sys

   if getattr(sys, 'oxidized', False):
       print('running in PyOxidizer!')
