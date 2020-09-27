.. _packaging_resources:

======================================
Managing Resources and Their Locations
======================================

An important concept in PyOxidizer packaging is how to manage
*resources* and their *locations*.

A *resource* is some entity that will be packaged or distributed. Examples
of *resources* include Python module bytecode, Python extension modules, and
arbitrary files on the filesystem.

A *location* is where that resource will be placed. Examples of *locations*
included *embedded in the built binary* and *in a file next to the built
binary*.

*Resources* are typically represented by a dedicated Starlark type. Locations
are typically expressed through a function name.

.. _packaging_resource_types:

Resource Types
==============

The following Starlark types represent individual resources:

:ref:`PythonSourceModule <config_python_source_module>`
   Source code for a Python module. Roughly equivalent to a ``.py`` file.

:ref:`PythonExtensionModule <config_python_extension_module>`
   A Python module defined through compiled, machine-native code. On Linux,
   these are typically encountered as ``.so`` files. On Windows, ``.pyd`` files.

:ref:`PythonPackageResource <config_python_package_resource>`
   A non-module *resource file* loadable by Python resources APIs, such as
   those in ``importlib.resources``.

:ref:`PythonPackageDistributionResource <config_python_package_distribution_resource>`
   A non-module *resource file* defining metadata for a Python package.
   Typically accessed via ``importlib.metadata``. This is how files in
   ``*.dist-info`` or ``*.egg-info`` directories are represented.

:ref:`FileContent <config_file_content>`
   Represents the content of a filesystem file.

There are also Starlark types that are logically containers for multiple
resources:

:ref:`FileManifest <config_file_manifest>`
   Holds a mapping of relative filesystem paths to ``FileContent`` instances.
   This type effectively allows modeling a directory tree.

:ref:`PythonEmbeddedResources <config_python_embedded_resources>`
   Holds a collection of Python resources of various types. (This type is often
   hidden away. e.g. inside a ``PythonExecutable`` instance.)

.. _packaging_resource_locations:

Python Resource Locations
=========================

The ``PythonEmbeddedResources`` type represents a collection of Python
resources of varying *resource* types and locations. When adding a Python
resource to this type, you have the choice of multiple locations for the
resource.

In-Memory
---------

When a Python resource is placed in the *in-memory* location, the content
behind the resource will be embedded in a built binary and loaded from there
by the Python interpreter.

Python modules imported from memory do not have the ``__file__`` attribute
set. This can cause compatibility issues if Python code is relying on the
existence of this module. See :ref:`no_file` for more.

Filesystem-Relative
-------------------

When a Python resource is placed in the *filesystem-relative* location,
the resource will be materialized as a file next to the produced entity.
e.g. a *filesystem-relative* ``PythonSourceModule`` for the ``foo.bar``
Python module added to a ``PythonExecutable`` will be materialized as the
file ``foo/bar.py`` or ``foo/bar/__init__.py`` in a directory next to the
built executable.

Resources added to *filesystem-relative* locations should be materialized
under paths that preserve semantics with standard Python file layouts. For
e.g. Python source and bytecode modules, it should be possible to point
``sys.path`` of any Python interpreter at the destination directory and
the modules will be loadable.

During packaging, PyOxidizer *indexes* all *filesystem-relative* resources
and embeds metadata about them in the built binary. While the files on the
filesystem may look like a standard Python install layout, loading them is
serviced by PyOxidizer's custom importer, not the standard importer that
Python uses by default.

Python Resource Location Policies
=================================

When constructing a Starlark type that represents a collection of Python
resources, the caller can specify a *policy* for what *locations* are
allowed and how to handle a resource if no explicit *location* is specified.
See :ref:`config_python_resources_policy` for the full documentation.

Here are some examples of how policies are used:

.. code-block:: python

   def make_exe():
       dist = default_python_distribution()

       policy = dist.make_python_packaging_policy()
       policy.resources_policy = "in-memory-only"

       # Only allow resources to be added to the in-memory location.
       exe = dist.to_python_executable(
           name = "myapp",
           packaging_policy = policy,
       )

       # Only allow resources to be added to the filesystem-relative location under
       # a "lib" directory.

       policy = dist.make_python_packaging_policy()
       policy.resources_policy = "filesystem-relative-only:lib"

       exe = dist.to_python_executable(
           name = "myapp",
           packaging_policy = policy,
       )

       # Try to add resources to in-memory first. If that fails, add them to a
       # "lib" directory relative to the built executable.

       policy = dist.make_python_packaging_policy()
       policy.resources_policy = "prefer-in-memory-fallback-filesystem-relative:lib"

       exe = dist.to_python_executable(
           name = "myapp",
           packaging_policy = policy,
       )

       return exe

.. _packaging_routing_resources:

Routing Python Resources to Locations
=====================================

Python resources are registered with built binaries by adding them to
the corresponding builder. For example, to add ``PythonSourceModule``
found by invoking a ``pip install`` command:

.. code-block:: python

   def make_exe():
       dist = default_python_distribution()

       policy = dist.make_python_packaging_policy()
       policy.resources_policy = "prefer-in-memory-fallback-filesystem-relative:lib"

       exe = dist.to_python_executable(
           name = "myapp",
           packaging_policy = policy,
       )

       for resource in exe.pip_install(["my-package"]):
           if type(resource) == "PythonSourceModule":
               # Location defined by policy.
               exe.add_python_resource(resource)
               # Force a location.
               resource.add_location = "in-memory"
               exe.add_python_resource(resource)
               resource.add_location = "filesystem-relative:lib"
               exe.add_python_resource(resource)

The methods for adding resources are
:ref:`config_python_executable_add_python_resource` and
:ref:`config_python_executable_add_python_resources`. The former adds a single
object. The latter an iterable of objects.

The method by which resources are added and loaded at run-time is
influenced by attributes on each Python resource object. See
:ref:`config_resource_locations` for more

.. _python_extension_module_location_compatibility:

``PythonExtensionModule`` Location Compatibility
================================================

Many resources *just work* in any available location. This is not the case for
``PythonExtensionModule`` instances!

While there only exists a single ``PythonExtensionModule`` type to represent
Python extension modules, Python extension modules come in various flavors.
Examples of flavors include:

* A module that is part of a Python *distribution* and is compiled into
  ``libpython`` (a *builtin* extension module).
* A module that is part of a Python *distribution* that is compiled as a
  standalone shared library (e.g. a ``.so`` or ``.pyd`` file).
* A non-*distribution* module that is compiled as a standalone shared library.
* A non-*distribution* module that is compiled as a static library.

Not all extension module *flavors* are compatible with all Python
*distributions*. Furthermore, not all *flavors* are compatible with all
build configurations.

Here are some of the rules governing extension modules and their locations:

* A *builtin* extension module that's part of a Python *distribution* will
  always be statically linked into ``libpython``.
* A Windows Python distribution with a statically linked ``libpython``
  (e.g. the ``standalone_static`` *distribution flavor*) is not capable
  of loading extension modules defined as shared libraries and only supports
  loading *builtin* extension modules statically linked into the binary.
* A Windows Python distribution with a dynamically linked ``libpython``
  (e.g. the ``standalone_dynamic`` *distribution flavor*) is capable of
  loading shared library backed extension modules from the *in-memory*
  location. Other operating systems do not support the *in-memory* location
  for loading shared library extension modules.
* If the current build configuration targets Linux MUSL-libc, shared library
  extension modules are not supported and all extensions must be statically
  linked into the binary.
* If the object files for the extension module are available, the extension
  module may be statically linked into the produced binary.
* If loading extension modules from in-memory import is supported, the
  extension module will have its dynamic library embedded in the binary.
* The extension module will be materialized as a file next to the produced
  binary and will be loaded from the filesystem. (This is how Python
  extension modules typically work.)

.. note::

   Extension module handling is one of the more nuanced aspects of PyOxidizer.
   There are likely many subtle bugs and room for improvement. If you
   experience problems handling extension modules, please consider
   `filing an issue <https://github.com/indygreg/PyOxidizer/issues>`_.
