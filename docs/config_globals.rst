.. _config_globals:

==============
Global Symbols
==============

This document lists every single global type, variable, and
function available in PyOxidizer's Starlark execution environment.

The Starlark environment contains symbols from the following:

* `Starlark built-ins <https://github.com/bazelbuild/starlark/blob/master/spec.md#built-in-constants-and-functions>`_
* :ref:`Tugger's Starlark Dialect <tugger_starlark>`
* PyOxidizer's Dialect (documented below)

.. _config_global_types:

Global Types
============

PyOxidizer's Starlark dialect defines the following custom types:

:ref:`config_type_file`
   Represents a filesystem path and content.

:ref:`tugger_starlark_type_file_content`
   Represents the content of a file on the filesystem.

   (Unlike :ref:`config_type_file`, this does not track the filename
   internally.)

:ref:`tugger_starlark_type_file_manifest`
   Represents a mapping of filenames to file content.

:ref:`config_type_python_distribution`
   Represents an implementation of Python.

   Used for embedding into binaries and running Python code.

:ref:`config_type_python_embedded_resources`
   Represents resources made available to a Python interpreter.

:ref:`config_type_python_executable`
   Represents an executable file containing a Python interpreter.

:ref:`config_type_python_extension_module`
   Represents a compiled Python extension module.

:ref:`config_type_python_interpreter_config`
   Represents the configuration of a Python interpreter.

:ref:`config_type_python_package_distribution_resource`
   Represents a file containing Python package distribution metadata.

:ref:`config_type_python_package_resource`
   Represents a non-module *resource* data file.

:ref:`config_type_python_packaging_policy`
   Represents a policy controlling how Python resources are added to a binary.

:ref:`config_type_python_module_source`
   Represents a ``.py`` file containing Python source code.

.. _config_global_constants:

Global Constants
================

The Starlark execution environment defines various variables in the
global scope which are intended to be used as read-only constants.
The following sections describe these variables.

.. _config_build_target_triple:

``BUILD_TARGET_TRIPLE``
-----------------------

The string Rust target triple that we're currently building for. Will be
a value like ``x86_64-unknown-linux-gnu`` or ``x86_64-pc-windows-msvc``.
Run ``rustup target list`` to see a list of targets.

.. _config_config_path:

``CONFIG_PATH``
---------------

The string path to the configuration file currently being evaluated.

.. _config_context:

``CONTEXT``
-----------

Holds build context. This is an internal variable and accessing it will
not provide any value.

.. _config_cwd:

``CWD``
-------

The current working directory. Also the directory containing the active
configuration file.

.. _config_global_functions:

Global Functions
================

PyOxidizer's Starlark dialect defines the following global functions:

:any:`default_python_distribution() <config_default_python_distribution>`
   Obtain the default :ref:`config_type_python_distribution`
   for the active build configuration.

:any:`register_target() <config_register_target>`
   Register a named :ref:`target <config_processing_targets>` that can
   be built.

:any:`resolve_target() <config_resolve_target>`
   Build/resolve a specific named :ref:`target <config_processing_targets>`.

:any:`resolve_targets() <config_resolve_targets>`
   Triggers resolution of requested build
   :ref:`targets <config_processing_targets>`.

:any:`set_build_path() <config_set_build_path>`
   Set the filesystem path to use for writing files during evaluation.

.. _config_types_with_target_behavior:

Types with Target Behavior
==========================

As described in :ref:`config_processing_targets`, a function registered
as a named target can return a type that has special *build* or *run*
behavior.

The following types have special behavior registered:

:ref:`tugger_starlark_type_file_manifest`
   Build behavior is to materialize all files in the file manifest.

   Run behavior is to run the last added :ref:`config_type_python_executable`
   if available, falling back to an executable file installed by the manifest
   if there is exactly 1 executable file.

:ref:`config_type_python_embedded_resources`
   Build behavior is to write out files this type represents.

   There is no run behavior.

:ref:`config_type_python_executable`
   Build behavior is to build the executable file.

   Run behavior is to run that built executable.
