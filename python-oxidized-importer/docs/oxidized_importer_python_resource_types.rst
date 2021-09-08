.. py:currentmodule:: oxidized_importer

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

:py:attr:`OxidizedResource.is_module`
   A Python module. These typically have source or bytecode attached.

   Modules can also be packages. In this case, they can hold additional
   data, such as a mapping of resource files.

:py:attr:`OxidizedResource.is_builtin_extension_module`
   A built-in extension module. These represent Python extension modules
   that are compiled into the application and don't exist as separate
   shared libraries.

:py:attr:`OxidizedResource.is_frozen_module`
   A frozen Python module. These are Python modules whose bytecode is
   compiled into the application.

:py:attr:`OxidizedResource.is_extension_module`
   A Python extension module. These are shared libraries that can be loaded
   to provide additional modules to Python.

:py:attr:`OxidizedResource.is_shared_library`
   A shared library. e.g. a ``.so`` or ``.dll``.

``PythonModuleSource``
======================

The :py:class:`PythonModuleSource` type represents Python module
source code. e.g. a ``.py`` file. See its linked API documentation
for more.

``PythonModuleBytecode``
========================

The :py:class:`PythonModuleBytecode` type represents Python
module bytecode. e.g. what a ``.pyc`` file holds (but without the header
that a ``.pyc`` file has).

``PythonExtensionModule``
=========================

The :py:class:`PythonExtensionModule` type represents a
Python extension module. This is a shared library defining a Python
extension implemented in native machine code that can be loaded into
a process and defines a Python module. Extension modules are typically
defined by ``.so``, ``.dylib``, or ``.pyd`` files.

.. note::

   Properties of this type are read-only.

``PythonPackageResource``
=========================

The :py:class:`PythonPackageResource` type represents a non-module
*resource* file.

``PythonPackageDistributionResource``
=====================================

The :py:class:`PythonPackageDistributionResource` type represents
a non-module *resource* file living in a package distribution directory
