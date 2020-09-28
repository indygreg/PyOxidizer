.. _config_globals:

=================================
Configuration File Global Symbols
=================================

This document lists every single global type, variable, and
function available in PyOxidizer's Starlark execution environment.

In addition to the symbols provided by PyOxidizer's Starlark
dialect, there are also the
`Starlark built-ins <https://github.com/bazelbuild/starlark/blob/master/spec.md#built-in-constants-and-functions>`_.

.. _config_global_types:

Global Types
============

PyOxidizer's Starlark dialect defines the following custom types:

:any:`FileContent <config_type_file_content>`
   Represents the content of a file on the filesystem.

:any:`FileManifest <config_type_file_manifest>`
   Represents a mapping of filenames to file content.

:ref:`config_type_python_distribution`
   Represents an implementation of Python.

   Used for embedding into binaries and running Python code.

:any:`PythonEmbeddedResources <config_type_python_embedded_resources>`
   Represents resources made available to a Python interpreter.

:any:`PythonExecutable <config_type_python_executable>`
   Represents an executable file containing a Python interpreter.

:any:`PythonExtensionModule <config_type_python_extension_module>`
   Represents a compiled Python extension module.

:any:`PythonInterpreterConfig <config_type_python_interpreter_config>`
   Represents the configuration of a Python interpreter.

:ref:`config_type_python_package_distribution_resource`
   Represents a file containing Python package distribution metadata.

:ref:`config_type_python_package_resource`
   Represents a non-module *resource* data file.

:any:`PythonPackagingPolicy <config_type_python_packaging_policy>`
   Represents a policy controlling how Python resources are added to a binary.

:ref:`config_type_python_source_module`
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

:any:`glob() <config_glob>`
   Collect files from the filesystem.

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