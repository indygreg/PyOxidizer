.. _tugger_starlark_globals:

==============
Global Symbols
==============

This document lists every single global type, variable, and
function available in Tugger's Starlark execution environment.

The Starlark environment contains symbols from the following:

* `Starlark built-ins <https://github.com/bazelbuild/starlark/blob/master/spec.md#built-in-constants-and-functions>`_
* Tugger's Dialect (documented below)

.. _tugger_starlark_global_types:

Global Types
============

Tugger's Starlark dialect defines the following custom types:

:ref:`tugger_starlark_type_file_content`
   Represents the content of a file on the filesystem.

:ref:`tugger_starlark_type_file_manifest`
   Represents a mapping of filenames to file content.

:ref:`tugger_starlark_type_snap_app`
   Represents an application inside a ``snapcraft.yaml`` file.

:ref:`tugger_starlark_type_snap_part`
   Represents a part inside a ``snapcraft.yaml`` file.

:ref:`tugger_starlark_type_snap`
   Represents a ``snapcraft.yaml`` file.

:ref:`tugger_starlark_type_wix_installer`
   Produce a Windows installer using WiX.

.. _tugger_starlark_global_functions:

Global Functions
================

Tugger's Starlark dialect defines the following global functions:

:ref:`tugger_starlark_glob`
   Collect files from the filesystem.
