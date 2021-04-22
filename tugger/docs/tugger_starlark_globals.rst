.. py:currentmodule:: starlark_tugger

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

:py:class:`FileContent`
   Represents the content of a file on the filesystem.

:py:class:`FileManifest`
   Represents a mapping of filenames to file content.

:py:class:`MacOsApplicationBundleBuilder`
   Used to create macOS Application Bundles (i.e. ``.app`` directories).

:py:class:`SnapApp`
   Represents an application inside a ``snapcraft.yaml`` file.

:py:class:`SnapPart`
   Represents a part inside a ``snapcraft.yaml`` file.

:py:class:`Snap`
   Represents a ``snapcraft.yaml`` file.

:ref:`tugger_starlark_type_snapcraft_builder`
   Manages the environment and invocations of the ``snapcraft`` command.

:ref:`tugger_starlark_type_wix_bundle_builder`
   Produce a Windows exe installer containing multiple installers using WiX.

:ref:`tugger_starlark_type_wix_installer`
   Produce a Windows installer using WiX.

:ref:`tugger_starlark_type_wix_msi_builder`
   Produce a Windows MSI installer with common installer features using WiX.

.. _tugger_starlark_global_functions:

Global Functions
================

Tugger's Starlark dialect defines the following global functions:

:ref:`tugger_starlark_glob`
   Collect files from the filesystem.
