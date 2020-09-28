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

.. _packaging_resource_packaging_policy:

Packaging Policies and Adding Resources
=======================================

The exact mechanism by which *resources* are added to *resource
collectors* is influenced by a *packaging policy* (represented by the
:ref:`PythonPackagingPolicy <config_python_packaging_policy>` Starlark
type) and attributes on each resource object influencing how they are
added.

When a *resource* is created, the *packaging policy* associated with
the entity creating the *resource* is applied and various ``add_*``
attributes on the Starlark *resource* types are populated.

When a resource is added (e.g. by calling
``PythonExecutable.add_python_resource()``), these attributes are
consulted and used to influence exactly how that *resource* is
added/packaged.

For example, a :ref:`config_python_source_module` can set attributes
indicating to exclude source code and only generate bytecode at
a specific optimization level. Or a :ref:`config_python_extension_module`
can set attributes saying to prefer to compile it into the built
binary or materialize it as a standalone dynamic extension module
(e.g. ``my_ext.so`` or ``my_ext.pyd``).

.. _packaging_resource_types:

Resource Types
==============

The following Starlark types represent individual resources:

:ref:`PythonSourceModule <config_python_source_module>`
   Source code for a Python module. Roughly equivalent to a ``.py`` file.

   This type can also be converted to Python bytecode (roughly equivalent
   to a ``.pyc``) when added to a resource collector.

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

Resource Attributes Influencing Adding
======================================

Individual Starlark values representing resources expose various
attributes prefixed with ``add_`` which influence what happens when
that resource is added to a resource collector. These attributes are
derived from the ``PythonPackagingPolicy`` attached to the entity
creating the resource. But they can be modified by Starlark code
before the resource is added to a collection.

The following sections describe each attribute that influences
how the resource is added to a collection.

.. _config_resource_add_include:

``add_include``
---------------

This ``bool`` attribute defines a yes/no filter for whether to actually
add this resource to a collection. If a resource with ``.add_include = False``
is added to a collection, that add is processed as a no-op and no change
is made.

.. _config_resource_add_location:

``add_location``
----------------

This ``string`` attributes defines the primary location this resource
should be added to and loaded from at run-time.

It can be set to the following values:

``in-memory``
   The resource should be loaded from memory.

   For Python modules and resource files, the module is loaded from
   memory using 0-copy by the custom module importer.

   For Python extension modules, the extension module may be statically
   linked into the built binary or loaded as a shared library from
   memory (the latter is not supported on all platforms).

``filesystem-relative:<prefix>``
   The resource is materialized on the filesystem relative to the built
   entity and loaded from the filesystem at run-time.

   ``<prefix>`` here is a directory prefix to place the resource in.
   ``.`` (e.g. ``filesystem-relative:.``) can be used to denote the same
   directory as the built entity.

.. _config_resource_add_location_fallback:

``add_location_fallback``
-------------------------

This ``string`` or ``None`` value attribute is equivalent to
``add_location`` except it only comes into play if the location
specified by ``add_location`` could not be satisfied.

Some resources (namely Python extension modules) cannot exist in
all locations. Setting this attribute to a different location gives
more flexibility for packaging resources with location constraints.

.. _config_resource_add_source:

``add_source``
--------------

This ``bool`` attribute defines whether to add source code for a
Python module.

For Python modules, typically only bytecode is required at run-time.
For some applications, the presence of source code doesn't provide
sufficient value or isn't desired since the application developer may
want to obfuscate the source code. Setting this attribute to ``False``
prevents Python module source code from being added.

.. _config_resource_add_bytecode_optimize_level_zero:

``add_bytecode_optimization_level_zero``
----------------------------------------

This ``bool`` attributes defines whether to add Python bytecode
for optimization level 0 (the default optimization level).

If ``True``, Python source code will be compiled to bytecode at
build time.

The default value is whatever
``PythonPackagingPolicy.bytecode_optimize_level_zero`` is set to.

.. _config_resource_add_bytecode_optimize_level_one:

``add_bytecode_optimization_level_one``
---------------------------------------

This ``bool`` attributes defines whether to add Python bytecode for
optimization level 1.

The default value is whatever
``PythonPackagingPolicy.bytecode_optimize_level_one`` is set to.

.. _config_resource_add_bytecode_optimize_level_two:

``add_bytecode_optimization_level_two``
---------------------------------------

This ``bool`` attributes defines whether to add Python bytecode for
optimization level 2.

The default value is whatever
``PythonPackagingPolicy.bytecode_optimize_level_two`` is set to.

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
   state. See :ref:`config_python_packaging_policy` for the full
   list of object attributes and methods that can be set or called.
2. Registering a function that will be called whenever a resource
   is created. This enables custom Starlark code to perform
   arbitrarily complex logic to influence settings and enables
   application developers to devise packaging strategies more
   advanced than what PyOxidizer provides out-of-the-box.

The following sections give examples of customized packaging
policies.

.. _packaging_resource_default_resource_location:

Customizing Default Resource Locations
--------------------------------------

The ``PythonPackagingPolicy.resources_policy`` attribute defines a
string which defines the default values for the ``add_location``
and ``add_location_fallback`` attributes.

Here are how values map to different ``add_*`` attributes:

``resources_policy = "in-memory-only"``
   ``add_location = "in-memory"`` and ``add_location_fallback = None``.

   Only adding and loading resources from memory is supported. This
   setting can produce single file executables.

``resources_policy = "filesystem-relative-only:<prefix>"``
   ``add_location = "filesystem-relative:<prefix>"`` and
   ``add_location_fallback = None``.

   Only adding and loading resources from the filesystem is supported.
   As a special case, Python extension modules may be linked as built-in
   extensions as part of the built ``libpython``.

   The ``<prefix>`` component of the value denotes the directory prefix
   that resources should be materialized at, relative to the built entity.
   The special value ``.`` denotes the same directory as the built entity.

``resources_policy = "prefer-in-memory-fallback-filesystem-realtive:<prefix>``
   ``add_location = "in-memory"`` and
   ``add_location_fallback = "filesystem-relative:<prefix>"``

   An attempt is made to add and load a resource from memory. If that isn't
   supported, the resource will be materialized on the filesystem.

And here is how you would set this value in Starlark:

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
       if type(resource) in ("PythonSourceModule", "PythonPackageResource", "PythonPackageDistributionResource"):
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
