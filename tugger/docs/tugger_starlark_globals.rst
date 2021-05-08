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

:py:class:`AppleUniversalBinary`
   Represents a multi-architecture *universal* binary for Apple platforms.

:py:class:`CodeSigner`
   An entity capable of performing code signing.

:py:class:`CodeSigningRequest`
   Holds settings to influence code signing on a single entity.

:py:class:`FileContent`
   Represents the content of a file on the filesystem.

:py:class:`FileManifest`
   Represents a mapping of filenames to file content.

:py:class:`MacOsApplicationBundleBuilder`
   Used to create macOS Application Bundles (i.e. ``.app`` directories).

:py:class:`PythonWheelBuilder`
   Create Python wheels (`.whl` files) from settings and file content.

:py:class:`SnapApp`
   Represents an application inside a ``snapcraft.yaml`` file.

:py:class:`SnapPart`
   Represents a part inside a ``snapcraft.yaml`` file.

:py:class:`Snap`
   Represents a ``snapcraft.yaml`` file.

:py:class:`SnapcraftBuilder`
   Manages the environment and invocations of the ``snapcraft`` command.

:py:class:`WiXBundleBuilder`
   Produce a Windows exe installer containing multiple installers using WiX.

:py:class:`WiXInstaller`
   Produce a Windows installer using WiX.

:py:class:`WiXMSIBuilder`
   Produce a Windows MSI installer with common installer features using WiX.

.. _tugger_starlark_global_functions:

Global Functions
================

Tugger's Starlark dialect defines the following global functions:

:py:func:`glob`
   Collect files from the filesystem.
