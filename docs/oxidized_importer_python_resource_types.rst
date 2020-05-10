.. _oxidized_importer_python_resource_types:

===========================================
``oxidized_importer`` Python Resource Types
===========================================

The ``oxidized_importer`` module defines Python types beyond
``OxidizedFinder``. This page documents those types and their APIs.

.. important::

   All types are backed by Rust structs and all properties return copies
   of the data. This means that if you mutate a Python variable that was
   obtained from an instance's property, that mutation won't be reflected
   in the backing Rust struct.

.. _oxidized_resource:

``OxidizedResource``
====================

The ``OxidizedResource`` Python type represents a *resource* that is indexed
by a ``OxidizedFinder`` instance.

Each instance represents a named entity with associated metadata and data.
e.g. an instance can represent a Python module with associated source and
bytecode.

New instances can be constructed via ``OxidizedResource()``. This will return
an instance whose ``flavor = "none"`` and ``name = ""``. All properties will
be ``None`` or ``false``.

Properties
----------

The following properties/attributes exist on ``OxidizedResource`` instances:

``flavor``
   A ``str`` describing the *flavor* of this resource.
   See :ref:`oxidized_resource_flavors` for more.

``name``
   The ``str`` name of the resource.

``is_package``
   A ``bool`` indicating if this resource is a Python package.

``is_namespace_package``
   A ``bool`` indicating if this resource is a Python namespace package.

``in_memory_source``
   ``bytes`` or ``None`` holding Python module source code that should be
   imported from memory.

``in_memory_bytecode``
   ``bytes`` or ``None`` holding Python module bytecode that should be
   imported from memory.

   This is raw Python bytecode, as produced from the ``marshal`` module.
   ``.pyc`` files have a header before this data that will need to be
   stripped should you want to move data from a ``.pyc`` file into this
   field.

``in_memory_bytecode_opt1``
   ``bytes`` or ``None`` holding Python module bytecode at optimization level 1
   that should be imported from memory.

   This is raw Python bytecode, as produced from the ``marshal`` module.
   ``.pyc`` files have a header before this data that will need to be
   stripped should you want to move data from a ``.pyc`` file into this
   field.

``in_memory_bytecode_opt2``
   ``bytes`` or ``None`` holding Python module bytecode at optimization level 2
   that should be imported from memory.

   This is raw Python bytecode, as produced from the ``marshal`` module.
   ``.pyc`` files have a header before this data that will need to be
   stripped should you want to move data from a ``.pyc`` file into this
   field.

``in_memory_extension_module_shared_library``
   ``bytes`` or ``None`` holding native machine code defining a Python extension
   module shared library that should be imported from memory.

``in_memory_package_resources``
   ``dict[str, bytes]`` or ``None`` holding resource files to make available to
   the ``importlib.resources`` APIs via in-memory data access. The ``name`` of
   this object will be a Python package name. Keys in this dict are virtual
   filenames under that package. Values are raw file data.

``in_memory_distribution_resources``
   ``dict[str, bytes]`` or ``None`` holding resource files to make available to
   the ``importlib.metadata`` API via in-memory data access. The ``name`` of
   this object will be a Python package name. Keys in this dict are virtual
   filenames. Values are raw file data.

``in_memory_shared_library``
   ``bytes`` or ``None`` holding a shared library that should be imported from
   memory.

``shared_library_dependency_names``
   ``list[str]`` or ``None`` holding the names of shared libraries that this
   resource depends on. If this resource defines a loadable shared library,
   this list can be used to express what other shared libraries it depends on.

``relative_path_module_source``
   ``pathlib.Path`` or ``None`` holding the relative path to Python module
   source that should be imported from the filesystem.

``relative_path_module_bytecode``
   ``pathlib.Path`` or ``None`` holding the relative path to Python module
   bytecode that should be imported from the filesystem.

``relative_path_module_bytecode_opt1``
   ``pathlib.Path`` or ``None`` holding the relative path to Python module
   bytecode at optimization level 1 that should be imported from the filesystem.

``relative_path_module_bytecode_opt1``
   ``pathlib.Path`` or ``None`` holding the relative path to Python module
   bytecode at optimization level 2 that should be imported from the filesystem.

``relative_path_extension_module_shared_library``
   ``pathlib.Path`` or ``None`` holding the relative path to a Python extension
   module that should be imported from the filesystem.

``relative_path_package_resources``
   ``dict[str, pathlib.Path]`` or ``None`` holding resource files to make
   available to the ``importlib.resources`` APIs via filesystem access. The
   ``name`` of this object will be a Python package name. Keys in this dict are
   filenames under that package. Values are relative paths to files from which
   to read data.

``relative_path_distribution_resources``
   ``dict[str, pathlib.Path]`` or ``None`` holding resource files to make
   available to the ``importlib.metadata`` APIs via filesystem access. The
   ``name`` of this object will be a Python package name. Keys in this dict are
   filenames under that package. Values are relative paths to files from which
   to read data.


.. _oxidized_resource_flavors:

``OxidizedResource`` Flavors
----------------------------

Each ``OxidizedResource`` instance describes a particular type of resource.
The type is indicated by a ``flavor`` property on the instance.

The following flavors are defined:

``none``
   There is no resource flavor (you shouldn't see this).

``module``
   A Python module. These typically have source or bytecode attached.

   Modules can also be packages. In this case, they can hold additional
   data, such as a mapping of resource files.

``built-in``
   A built-in extension module. These represent Python extension modules
   that are compiled into the application and don't exist as separate
   shared libraries.

``frozen``
   A frozen Python module. These are Python modules whose bytecode is
   compiled into the application.

``extension``
   A Python extension module. These are shared libraries that can be loaded
   to provide additional modules to Python.

``shared_library``
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

   With ``OxidizedFinder``, an ``importlib.abc.ResourceReader`` associated
   with this package will be used to load the resource.

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
