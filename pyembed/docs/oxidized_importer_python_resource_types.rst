.. _oxidized_importer_python_resource_types:

===========================================
``oxidized_importer`` Python Resource Types
===========================================

The ``oxidized_importer`` module defines Python types beyond
:py:class:`OxidizedFinder`. This page documents those types and their APIs.

.. important::

   All types are backed by Rust structs and all properties return copies
   of the data. This means that if you mutate a Python variable that was
   obtained from an instance's property, that mutation won't be reflected
   in the backing Rust struct.

.. _oxidized_resource:

``OxidizedResource``
====================

Represents a *resource* that is indexed by a :py:class:`OxidizedFinder`
instance.

See :py:class:`OxidizedResource` for API documentation.

.. _oxidized_resource_flavors:

``OxidizedResource`` Resource Types
-----------------------------------

Each ``OxidizedResource`` instance describes a particular type of resource.
If a resource identifies as a type, it sets one of the following ``is_*``
attributes to ``True``:

``is_module``
   A Python module. These typically have source or bytecode attached.

   Modules can also be packages. In this case, they can hold additional
   data, such as a mapping of resource files.

``is_builtin_extension_module``
   A built-in extension module. These represent Python extension modules
   that are compiled into the application and don't exist as separate
   shared libraries.

``is_frozen_module``
   A frozen Python module. These are Python modules whose bytecode is
   compiled into the application.

``is_extension_module``
   A Python extension module. These are shared libraries that can be loaded
   to provide additional modules to Python.

``is_shared_library``
   A shared library. e.g. a ``.so`` or ``.dll``.

``PythonModuleSource``
======================

The ``oxidized_importer.PythonModuleSource`` type represents Python module
source code. e.g. a ``.py`` file.

Instances have the following properties:

``module`` (``str``)
   The fully qualified Python module name. e.g. ``my_package.foo``.

``source`` (``bytes``)
   The source code of the Python module.

   Note that source code is stored as ``bytes``, not ``str``. Most Python
   source is stored as ``utf-8``, so you can ``.encode("utf-8")`` or
   ``.decode("utf-8")`` to convert between ``bytes`` and ``str``.

``is_package`` (``bool``)
   This this module is a Python package.

``PythonModuleBytecode``
========================

The ``oxidized_importer.PythonModuleBytecode`` type represents Python
module bytecode. e.g. what a ``.pyc`` file holds (but without the header
that a ``.pyc`` file has).

Instances have the following properties:

``module`` (``str``)
   The fully qualified Python module name.

``bytecode`` (``bytes``)
   The bytecode of the Python module.

   This is what you would get by compiling Python source code via
   something like ``marshal.dumps(compile(source, "exe"))``. The bytecode
   does **not** contain a header, like what would be found in a ``.pyc``
   file.

``optimize_level`` (``int``)
   The bytecode optimization level. Either ``0``, ``1``, or ``2``.

``is_package`` (``bool``)
   Whether this module is a Python package.

``PythonExtensionModule``
=========================

The ``oxidized_importer.PythonExtensionModule`` type represents a
Python extension module. This is a shared library defining a Python
extension implemented in native machine code that can be loaded into
a process and defines a Python module. Extension modules are typically
defined by ``.so``, ``.dylib``, or ``.pyd`` files.

Instances have the following properties:

``name`` (``str``)
   The name of the extension module.

.. note::

   Properties of this type are read-only.

``PythonPackageResource``
=========================

The ``oxidized_importer.PythonPackageResource`` type represents a non-module
*resource* file. These are files that live next to Python modules that
are typically accessed via the APIs in ``importlib.resources``.

Instances have the following properties:

``package`` (``str``)
   The name of the leaf-most Python package this resource is associated with.

   With :py:class:`OxidizedFinder`, an ``importlib.abc.ResourceReader``
   associated with this package will be used to load the resource.

``name`` (``str``)
   The name of the resource within its ``package``. This is typically the
   filename of the resource. e.g. ``resource.txt`` or ``child/foo.png``.

``data`` (``bytes``)
   The raw binary content of the resource.

``PythonPackageDistributionResource``
=====================================

The ``oxidized_importer.PythonPackageDistributionResource`` type represents
a non-module *resource* file living in a package distribution directory
(e.g. ``<package>-<version>.dist-info`` or ``<package>-<version>.egg-info``).
These resources are typically accessed via the APIs in ``importlib.metadata``.

Instances have the following properties:

``package`` (``str``)
   The name of the Python package this resource is associated with.

``version`` (``str``)
   Version string of Python package this resource is associated with.

``name`` (``str``)
   The name of the resource within the metadata distribution. This is
   typically the filename of the resource. e.g. ``METADATA``.

``data`` (``bytes``)
   The raw binary content of the resource.
