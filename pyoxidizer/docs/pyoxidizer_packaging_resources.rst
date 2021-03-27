.. _packaging_resources:

================================
Managing How Resources are Added
================================

An important concept in PyOxidizer packaging is how to manage *resources*
that are added to built applications.

A *resource* is some entity that will be packaged and distributed. Examples
of *resources* include Python module source and bytecode, Python
extension modules, and arbitrary files on the filesystem.

*Resources* are represented by a dedicated Starlark type for each
resource flavor (see :ref:`packaging_resource_types`).

During evaluation of PyOxidizer's Starlark configuration files,
*resources* are created and *added* to another Starlark type whose
job is to collect all desired *resources* and then do something with
them.

.. _packaging_resources_classified_files:

Classified Resources Versus Files
=================================

All resources in PyOxidizer are ultimately derived from or representable
by a file or a file-like primitive. For example, a
:ref:`config_type_python_module_source` is derived from or could be
manifested as a ``.py`` file.

Various PyOxidizer functionality works by scanning existing files and
turning those files into *resources*.

This file scanning functionality has two modes of operation: *classified*
and *files*. In *files* mode, PyOxidizer simply emits resources corresponding
to the raw files it encounters. In *classified* mode, PyOxidizer attempts to
*classify* a file as a particular resource and emit a strongly-typed
resource like :ref:`config_type_python_module_source` or
:ref:`config_type_python_extension_module`.

*Classified* mode is more powerful because PyOxidizer is able to build
an *index* of typed resources at packaging time and make this *index*
available to :ref:`oxidized_importer` at run-time to facilitate faster
loading of resources.

However, the main downside to *classified* mode is it relies on being able
to identify files properly and this is unreliable. Python file layouts are
under-specified and there are many edge cases where PyOxidizer fails to
properly classify a file. See :ref:`cli_find_resources` for how to identify
problems here.

In *files* mode, PyOxidizer simply indexes and manages a named file
and its content. There is far less potential for PyOxidizer to make
mistakes about a file's type and how it is handled. This means that
*files* mode often *just works* when *classified* mode doesn't. The main
downside to *files* mode is that :ref:`oxidized_importer` doesn't have a
rich index embedded in the built binary, so you will have to rely on
Python's default filesystem-based importer, which is slower than
``oxidized_importer``.

.. _packaging_resource_packaging_policy:

Packaging Policies and Adding Resources
=======================================

The exact mechanism by which *resources* are emitted and added to *resource
collectors* is influenced by a *packaging policy* (represented by the
:ref:`config_type_python_packaging_policy` Starlark type) and attributes on
each resource object influencing how they are added.

When *resources* are created, the *packaging policy* determines whether
emitted resources are *classified* or simply *files*. And the *packaging
policy* is applied to each created resource to populate the initial values
for the various ``add_*`` attributes on the Starlark *resource* types.

When a resource is added (e.g. by calling
``PythonExecutable.add_python_resource()``), these aforementioned
``add_*`` attributes are consulted and used to influence exactly how that
*resource* is added/packaged.

For example, a :ref:`config_type_python_module_source` can set attributes
indicating to exclude source code and only generate bytecode at
a specific optimization level. Or a :ref:`config_type_python_extension_module`
can set attributes saying to prefer to compile it into the built
binary or materialize it as a standalone dynamic extension module
(e.g. ``my_ext.so`` or ``my_ext.pyd``).

.. _packaging_resource_types:

Resource Types
==============

The following Starlark types represent individual resources:

:ref:`config_type_python_module_source`
   Source code for a Python module. Roughly equivalent to a ``.py`` file.

   This type can also be converted to Python bytecode (roughly equivalent
   to a ``.pyc``) when added to a resource collector.

:ref:`config_type_python_extension_module`
   A Python module defined through compiled, machine-native code. On Linux,
   these are typically encountered as ``.so`` files. On Windows, ``.pyd`` files.

:ref:`config_type_python_package_resource`
   A non-module *resource file* loadable by Python resources APIs, such as
   those in ``importlib.resources``.

:ref:`config_type_python_package_distribution_resource`
   A non-module *resource file* defining metadata for a Python package.
   Typically accessed via ``importlib.metadata``. This is how files in
   ``*.dist-info`` or ``*.egg-info`` directories are represented.

:ref:`config_type_file`
   Represents a filesystem path and its content.

:ref:`tugger_starlark_type_file_content`
   Represents the content of a filesystem file.

   This is different from :ref:`config_type_file` in that it only
   represents file content and doesn't have an associated path. (It is
   likely these 2 types will be merged someday.)

There are also Starlark types that are logically containers for multiple
resources:

:ref:`tugger_starlark_type_file_manifest`
   Holds a mapping of relative filesystem paths to ``FileContent`` instances.
   This type effectively allows modeling a directory tree.

:ref:`config_type_python_embedded_resources`
   Holds a collection of Python resources of various types. (This type is often
   hidden away. e.g. inside a ``PythonExecutable`` instance.)

.. _packaging_resource_locations:

Resource Locations
==================

Resources have the concept of a *location*. A resource's *location*
determines where the data for that resource is packaged and how that
resource is loaded at run-time.

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
e.g. a *filesystem-relative* ``PythonModuleSource`` for the ``foo.bar``
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

.. _packaging_resource_custom_policies:

Customizing Python Packaging Policies
=====================================

As described in :ref:`packaging_resource_packaging_policy`, a
``PythonPackagingPolicy`` Starlark type instance is bound to every
entity creating *resource* instances and this *packaging policy* is
used to derive the default ``add_*`` attributes which influence
what happens when a resource is added to some entity.

``PythonPackagingPolicy`` instances can be customized to influence
what the default values of the ``add_*`` attributes are.

The primary mechanisms for doing this are:

1. Modifying the ``PythonPackagingPolicy`` instance's internal
   state. See :ref:`config_type_python_packaging_policy` for the full
   list of object attributes and methods that can be set or called.
2. Registering a function that will be called whenever a resource
   is created. This enables custom Starlark code to perform
   arbitrarily complex logic to influence settings and enables
   application developers to devise packaging strategies more
   advanced than what PyOxidizer provides out-of-the-box.

The following sections give examples of customized packaging
policies.

.. _packaging_resources_resources_mode:

Changing the Resource Handling Mode
-----------------------------------

As documented in :ref:`packaging_resources_classified_files`, PyOxidizer
can operate on *classified* resources or *files*-based resources.

:ref:`config_type_python_packaging_policy_set_resource_handling_mode`
exists to change the operating mode of a ``PythonPackagingPolicy``
instance.

.. code-block:: python

   def make_exe():
       dist = default_python_distribution()

       policy = dist.make_python_packaging_policy()

       # Set policy attributes to only operate on "classified" resource types.
       # (This is the default.)
       policy.set_resource_handling_mode("classify")

       # Set policy attributes to only operate on `File` resource types.
       policy.set_resource_handling_mode("files")

:ref:`config_type_python_packaging_policy_set_resource_handling_mode` is
just a convenience method for manipulating a collection of attributes on
``PythonPackagingPolicy`` instances. If you don't like the behavior of
its pre-defined modes, feel free to adjust attributes to suit your needs.
You can even configure things to emit both *classified* and *files*
variants simultaneously!

.. _packaging_resource_default_resource_location:

Customizing Default Resource Locations
--------------------------------------

The ``PythonPackagingPolicy.resources_location`` and
``PythonPackagingPolicy.resources_location_fallback`` attributes define
primary and fallback locations that resources should attempt to be added
to. These effectively define the default values for the ``add_location``
and ``add_location_fallback`` attributes on individual resource objects.

The accepted values are:

``in-memory``
   Load resources from memory.

``filesystem-relative:prefix``
   Load resources from the filesystem at a path relative to some entity
   (probably the binary being built).

Additionally, ``PythonPackagingPolicy.resources_location_fallback`` can be
set to ``None`` to remove a fallback location.

And here is how you would manage these values in Starlark:

.. code-block:: python

   def make_exe():
       dist = default_python_distribution()

       policy = dist.make_python_packaging_policy()
       policy.resources_location = "in-memory"
       policy.resources_location_fallback = None

       # Only allow resources to be added to the in-memory location.
       exe = dist.to_python_executable(
           name = "myapp",
           packaging_policy = policy,
       )

       # Only allow resources to be added to the filesystem-relative location under
       # a "lib" directory.

       policy = dist.make_python_packaging_policy()
       policy.resources_location = "filesystem-relative:lib"
       policy.resources_location_fallback = None

       exe = dist.to_python_executable(
           name = "myapp",
           packaging_policy = policy,
       )

       # Try to add resources to in-memory first. If that fails, add them to a
       # "lib" directory relative to the built executable.

       policy = dist.make_python_packaging_policy()
       policy.resources_location = "in-memory"
       policy.resources_location_fallback = "filesystem-relative:lib"

       exe = dist.to_python_executable(
           name = "myapp",
           packaging_policy = policy,
       )

       return exe

.. _packaging_resource_callback:

Using Callbacks to Influence Resource Attributes
------------------------------------------------

The ``PythonPackagingPolicy.register_resource_callback(func)`` method will
register a function to be called when resources are created. This function
receives as arguments the active ``PythonPackagingPolicy`` and the newly
created resource.

Functions registered as resource callbacks are called after the
``add_*`` attributes are derived for a resource but before the resource
is otherwise made available to other Starlark code. This means that
these callbacks provide a hook point where resources can be modified as
soon as they are created.

``register_resource_callback()`` can be called multiple times to register
multiple callbacks. Registered functions will be called in order of
registration.

Functions can be leveraged to unify all resource packaging logic in a
single place, making your Starlark configuration files easier to reason
about.

Here's an example showing how to route all resources belonging to
a single package to a ``filesystem-relative`` location and everything
else to memory:

.. code-block:: python

   def resource_callback(policy, resource):
       if type(resource) in ("PythonModuleSource", "PythonPackageResource", "PythonPackageDistributionResource"):
           if resource.package == "my_package":
               resource.add_location = "filesystem-relative:lib"
           else:
               resource.add_location = "in-memory"

   def make_exe():
       dist = default_python_distribution()

       policy = dist.make_python_packaging_policy()
       policy.register_resource_callback(resource_callback)

       exe = dist.to_python_executable(
           name = "myapp",
           packaging_policy = policy,
       )

       exe.add_python_resources(exe.pip_install(["my_package"]))

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
