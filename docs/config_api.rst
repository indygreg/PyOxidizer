.. _config_api:

================================
Configuration File API Reference
================================

This document describes the low-level API for ``PyOxidizer`` configuration
files. For a higher-level overview of how configuration files work, see
:ref:`config_files`.

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

.. _config_python_binaries:

Python Binaries
===============

Binaries containing an embedded Python interpreter can be defined by
configuration files. They are defined via the :ref:`config_type_python_executable`
type. In addition, the :ref:`config_type_python_embedded_resources` type represents
the collection of resources made available to an embedded Python interpreter.
