.. _packaging_extension_modules:

=====================================
Working with Python Extension Modules
=====================================

Python extension modules are machine native code exposing
functionality to a Python interpreter via Python modules.

PyOxidizer has varying levels of support for extension modules. This
is because some PyOxidizer configurations break assumptions about
how Python interpreters typically run.

This document attempts to capture all the nuances of working with
Python extension modules with PyOxidizer.

Extension Module Flavors
========================

Python extension modules exist as either *built-in* or *standalone*.
A *built-in* extension module is statically linked into *libpython*
and a *standalone* extension module is a shared library that is
dynamically loaded at run-time.

Typically, *built-in* extension modules only exist in Python
distributions (and are part of the Python standard library by definition)
and Python package maintainers only ever produce *standalone* extension
modules (e.g. as ``.so`` or ``.pyd`` files).

Python distributions typically contain a mix of *built-in* and
*standalone* extension modules. e.g. the ``_ast`` extension module is
*built-in* and the ``_ssl`` extension module is *standalone*.

.. important::

   Because PyOxidizer enables you to build your own binaries embedding
   Python and because different Python distributions have different
   levels of support for extension modules, it is important to familiarize
   yourself with the types of extension modules and how they can be used.

.. _packaging_extension_module_restrictions:

Extension Module Restrictions
=============================

PyOxidizer imposes a handful of restrictions on how extension modules
work. These restrictions are typically a side-effect of limitations
of the :ref:`Python distribution <packaging_python_distributions>` being
used/targeted. These restrictions are documented in the sections below.

.. _packaging_extension_modules_musl:

musl libc Linux Distributions Only Support Built-in Extension Modules
---------------------------------------------------------------------

The Python distributions built against musl libc (build target
``*-linux-musl``) only support *built-in* extension modules.

This is because musl libc binaries are statically linked and statically
linked Linux binaries are incapable of calling ``dlopen()`` to load a
shared library.

This means Python binaries built in this configuration cannot load
*standalone* Python extension modules existing as separate files (``.so``
files typically). This means PyOxidizer cannot consume Python wheels
or other Python resource sources containing pre-built Python extension
modules.

In order for PyOxidizer to support a Python extension module built for
musl libc, it must compile that extension module from source and link
the resulting object files / static library directly into the built
binary and expose that extension module as a *built-in*. This is done
using :ref:`packaging_distutils_hack`.

.. _packaging_extension_modules_windows_static:

Windows Static Distributions Only Support Built-in Extension Modules
--------------------------------------------------------------------

The Windows ``standalone_static`` distribution flavor only supports
*built-in* extension modules and doesn't support loading shared library
extension modules.

See the above section for implications on this.

The situation of having to rebuild Python extension modules on Windows
is often more complicated than on Linux because oftentimes building
extension modules on Windows isn't as trivial as on Linux. This is
because many Windows environments don't have the correct version of
Visual Studio or various library dependencies. If you want a turnkey
experience for Windows packaging, it is recommended to use the
``standalone_dynamic`` distribution flavor.

.. _packaging_extension_modules_in_memory:

Loading Extension Modules from ``in-memory`` Location
-----------------------------------------------------

When you attempt to add a :ref:`config_type_python_extension_module`
Starlark instance to the ``in-memory``
:ref:`resource location <packaging_resource_locations>`, the request
may or may not work depending on the state of the extension module
and support from the Python distribution.

The ``in-memory`` resource location is interpreted by PyOxidizer as
*load this extension from memory, without having a standalone file*.
PyOxidizer will try its hardest to satisfy this request.

If the object files / static library of an extension module are known
to PyOxidizer, these will be statically linked into the built binary
and the extension module will be exposed as a *built-in* extension
module.

If only a shared library is available for the extension module,
PyOxidizer only supports loading shared libraries from memory on
Windows ``standalone_dynamic`` distributions: in all other
platforms the request to load a shared library extension module is
rejected.

Some extensions and shared libraries are known to not work when
loaded from memory using the custom shared library loader used by
PyOxidizer. For this reason,
:ref:`config_type_python_packaging_policy_allow_in_memory_shared_library_loading`
exists to control this behavior.

.. important::

   Because the ``in-memory`` location for extension modules can be
   brittle, it is recommended to set a resources policy or
   ``add_location_fallback`` to allow extension modules to exist as
   standalone files. This will provide maximum compatibility with
   built Python extension modules and will reduce the complexity of
   packaging 3rd party extension modules.

.. _packaging_extension_module_library_dependencies:

Extension Module Library Dependencies
=====================================

PyOxidizer doesn't currently support resolving additional library
dependencies from discovered extension modules outside of the
Python distribution. For example, if your extension module ``foo.so``
has a run-time dependency on ``bar.so``, PyOxidizer doesn't yet
detect this and doesn't realize that ``bar.so`` needs to be handled.

This means that if you add a :ref:`config_type_python_extension_module`
Starlark type and this extension module depends on an additional
library, PyOxidizer will likely not realize this and fail to
distribute that additional library dependency with your application.

If your Python extensions depend on additional libraries, you may need
to manually add these files to your installation via custom
Starlark code.

Note that if your shared library exists as a file in Python package
(a directory with ``__init__.py`` somewhere in the hierarchy), PyOxidizer's
resource scanning may detect the shared library as a
:ref:`config_type_python_package_resource` and package this resource.
However, the packaged resource won't be flagged as a shared library.
This means that the run-time importer won't identify the shared library
dependency and won't take steps to ensure it is available/loaded before
the extension is loaded. This means that the shared library loading needs
to be handled by the operating system's default rules. And this means
that the shared library file must exist on the filesystem, next to a
file-based extension module.

.. _packaging_distutils_hack:

Building with a Custom Distutils
================================

If PyOxidizer is not able to reuse an existing shared library
extension module or the build configuration is forcing an extension
to be built as a *built-in*, PyOxidizer attempts to compile the
extension module from source so that it can be statically linked as
a *built-in*.

The way PyOxidizer achieves this is a bit crude, but often effective.

When PyOxidizer invokes ``pip`` or ``setup.py`` to build a package,
it installs a modified version of ``distutils`` into the invoked
Python's ``sys.path``. This modified ``distutils`` changes the
behavior of some key build steps (notably how C extensions are compiled)
such that the build emits artifacts that PyOxidizer can statically
link into a custom binary.

For example, on Linux, PyOxidizer copies the intermediate object files
produced by the build and links them into the binary containing the
generated ``libpython``. PyOxidizer completely ignores the shared
library that is or would typically be produced.

If ``setup.py`` scripts are following the traditional pattern of using
`distutils.core.Extension <https://docs.python.org/3/distutils/apiref.html#distutils.core.Extension>`_
to define extension modules, things tend to *just work* (assuming extension
modules are supported by PyOxidizer for the target platform). However,
if ``setup.py`` scripts are doing their own monkeypatching of
``distutils``, rely on custom build steps or types to compile extension
modules, or invoke separate Python processes to interact with ``distutils``,
things may break.

The easiest way to avoid the pitfalls of a custom ``distutils`` build
is to not attempt to produce a statically linked binary: use a
``standalone_dynamic`` distribution flavor that supports loading
extension modules from files.

Until PyOxidizer supports telling it additional object files or
static libraries to link into a binary, there's no easy workaround aside
from giving up on a statically linked binary. Better support will hopefully
be present in future versions of PyOxidizer.
