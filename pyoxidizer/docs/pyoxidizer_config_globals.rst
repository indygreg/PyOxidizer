.. py:currentmodule:: starlark_pyoxidizer

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

In addition, extra global variables can be injected into the execution
environment on a per-invocation basis. This is commonly encountered
with use of the ``--var`` and `--var-env`` arguments to various ``pyoxidizer``
sub-commands.

.. _config_global_types:

Global Types
============

PyOxidizer's Starlark dialect defines the following custom types:

:py:class:`File`
   Represents a filesystem path and content.

:py:class:`starlark_tugger.FileContent`
   Represents the content of a file on the filesystem.

   (Unlike :py:class:`File`, this does not track the filename
   internally.)

:py:class:`starlark_tugger.FileManifest`
   Represents a mapping of filenames to file content.

:py:class:`PythonDistribution`
   Represents an implementation of Python.

   Used for embedding into binaries and running Python code.

:py:class:`PythonEmbeddedResources`
   Represents resources made available to a Python interpreter.

:py:class:`PythonExecutable`
   Represents an executable file containing a Python interpreter.

:py:class:`PythonExtensionModule`
   Represents a compiled Python extension module.

:py:class:`PythonInterpreterConfig`
   Represents the configuration of a Python interpreter.

:py:class:`PythonPackageDistributionResource`
   Represents a file containing Python package distribution metadata.

:py:class:`PythonPackageResource`
   Represents a non-module *resource* data file.

:py:class:`PythonPackagingPolicy`
   Represents a policy controlling how Python resources are added to a binary.

:py:class:`PythonModuleSource`
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

:py:func:`default_python_distribution`
   Obtain the default :py:class:`PythonDistribution` for the active build configuration.

:any:`register_target() <config_register_target>`
   Register a named :ref:`target <config_processing_targets>` that can
   be built.

:any:`resolve_target() <config_resolve_target>`
   Build/resolve a specific named :ref:`target <config_processing_targets>`.

:any:`resolve_targets() <config_resolve_targets>`
   Triggers resolution of requested build
   :ref:`targets <config_processing_targets>`.

:py:func:`set_build_path`
   Set the filesystem path to use for writing files during evaluation.

.. _config_types_with_target_behavior:

Types with Target Behavior
==========================

As described in :ref:`config_processing_targets`, a function registered
as a named target can return a type that has special *build* or *run*
behavior.

The following types have special behavior registered:

:py:class:`starlark_tugger.FileManifest`
   Build behavior is to materialize all files in the file manifest.

   Run behavior is to run the last added :py:class:`PythonExecutable`
   if available, falling back to an executable file installed by the manifest
   if there is exactly 1 executable file.

:py:class:`PythonEmbeddedResources`
   Build behavior is to write out files this type represents.

   There is no run behavior.

:py:class:`PythonExecutable`
   Build behavior is to build the executable file.

   Run behavior is to run that built executable.
