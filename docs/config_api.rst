.. _config_api:

================================
Configuration File API Reference
================================

This document describes the low-level API for ``PyOxidizer`` configuration
files. For a higher-level overview of how configuration files work, see
:ref:`config_files`.

.. _config_resource_locations:

Specifying Resource Locations
=============================

Various functionality relates to the concept of a *resource location*, or
where a resource should be loaded from at run-time. See
:ref:`packaging_resources` for more.

Resource locations are represented as strings in Starlark. The mapping
of strings to resource locations is as follows:

``default``
   Use the default resource location. Often equivalent to a resource location
   of the type/value ``None``.

``in-memory``
   Load the resource from memory.

``filesystem-relative:<prefix>``
   Install and load the resource from a filesystem relative path to the
   build binary. e.g. ``filesystem-relative:lib`` will place resources
   in the ``lib/`` directory next to the build binary.

.. _config_python_resources:

Python Resources
================

At run-time, Python interpreters need to consult *resources* like Python
module source and bytecode as well as resource/data files. We refer to all
of these as *Python Resources*.

Configuration files represent *Python Resources* via the types
:ref:`config_type_python_source_module`,
:ref:`config_type_python_package_resource`,
:ref:`config_type_python_package_distribution_resource`,
and :ref:`config_type_python_extension_module`.

These are described in detail in the following sections.

.. _config_python_resources_policy:

Python Resources Policy
=======================

There are various ways to add resources (typically Python resources) to
a binary. For example, you can import modules from memory or the filesystem.
Often, configuration files may wish to be explicit about what behavior is
and is not allowed. A *Python Resources Policy* is used to apply said
behavior.

A *Python Resources Policy* is defined by a ``str``. The following
values are recognized.

``in-memory-only``
   Resources are to be loaded from in-memory only. If a resource cannot be
   loaded from memory (e.g. dynamically linked Python extension modules in
   some configurations), an error will (likely) occur.

``filesystem-relative-only:<prefix>``
   Values starting with ``filesystem-relative-only:`` specify that resources are
   to be loaded from the filesystem from paths relative to the produced
   binary. Files will be installed at the path prefix denoted by the value after
   the ``:``. e.g. ``filesystem-relative-only:lib`` will install resources in a
   ``lib/`` directory.

``prefer-in-memory-fallback-filesystem-relative:<prefix>``
   Values starting with ``prefer-in-memory-fallback-filesystem-relative`` represent
   a hybrid between ``in-memory-only`` and ``filesystem-relative-only:<prefix>``.
   Essentially, if in-memory resource loading is supported, it is used. Otherwise
   we fall back to loading from the filesystem from paths relative to the produced
   binary.

.. _config_python_binaries:

Python Binaries
===============

Binaries containing an embedded Python interpreter can be defined by
configuration files. They are defined via the :ref:`config_type_python_executable`
type. In addition, the :ref:`config_type_python_embedded_resources` type represents
the collection of resources made available to an embedded Python interpreter.

Interacting With the Filesystem
===============================

.. _config_glob:

``glob(include, exclude=None, strip_prefix=None)``
--------------------------------------------------

The ``glob()`` function resolves file patterns to a ``FileManifest``.

``include`` is a ``list`` of ``str`` containing file patterns that will be
matched using the ``glob`` Rust crate. If patterns begin with ``/`` or
look like a filesystem absolute path, they are absolute. Otherwise they are
evaluated relative to the directory of the current config file.

``exclude`` is an optional ``list`` of ``str`` and is used to exclude files
from the result. All patterns in ``include`` are evaluated before ``exclude``.

``strip_prefix`` is an optional ``str`` to strip from the beginning of
matched files. ``strip_prefix`` is stripped after ``include`` and ``exclude``
are processed.

Returns a ``FileManifest``.
