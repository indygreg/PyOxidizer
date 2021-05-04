.. py:currentmodule:: starlark_pyoxidizer

.. _config_resource_add_attributes:

======================================
Resource Attributes Influencing Adding
======================================

Individual Starlark values representing resources expose various
attributes prefixed with ``add_`` which influence what happens when
that resource is added to a resource collector. These attributes are
derived from the :py:class:`PythonPackagingPolicy` attached to
the entity creating the resource. But they can be modified by Starlark code
before the resource is added to a collection.

The following sections describe each attribute that influences
how the resource is added to a collection.

.. _config_resource_add_include:

``add_include``
===============

This ``bool`` attribute defines a yes/no filter for whether to actually
add this resource to a collection. If a resource with ``.add_include = False``
is added to a collection, that add is processed as a no-op and no change
is made.

.. _config_resource_add_location:

``add_location``
================

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
=========================

This ``string`` or ``None`` value attribute is equivalent to
``add_location`` except it only comes into play if the location
specified by ``add_location`` could not be satisfied.

Some resources (namely Python extension modules) cannot exist in
all locations. Setting this attribute to a different location gives
more flexibility for packaging resources with location constraints.

.. _config_resource_add_source:

``add_source``
==============

This ``bool`` attribute defines whether to add source code for a
Python module.

For Python modules, typically only bytecode is required at run-time.
For some applications, the presence of source code doesn't provide
sufficient value or isn't desired since the application developer may
want to obfuscate the source code. Setting this attribute to ``False``
prevents Python module source code from being added.

.. _config_resource_add_bytecode_optimize_level_zero:

``add_bytecode_optimization_level_zero``
========================================

This ``bool`` attributes defines whether to add Python bytecode
for optimization level 0 (the default optimization level).

If ``True``, Python source code will be compiled to bytecode at
build time.

The default value is whatever
:py:attr:`PythonPackagingPolicy.bytecode_optimize_level_zero` is set to.

.. _config_resource_add_bytecode_optimize_level_one:

``add_bytecode_optimization_level_one``
=======================================

This ``bool`` attributes defines whether to add Python bytecode for
optimization level 1.

The default value is whatever
:py:attr:`PythonPackagingPolicy.bytecode_optimize_level_one` is set to.

.. _config_resource_add_bytecode_optimize_level_two:

``add_bytecode_optimization_level_two``
=======================================

This ``bool`` attributes defines whether to add Python bytecode for
optimization level 2.

The default value is whatever
:py:attr:`PythonPackagingPolicy.bytecode_optimize_level_two` is set to.
