.. _tugger_wix:

===================================================
Using the WiX Toolset to Produce Windows Installers
===================================================

The `WiX Toolset <https://wixtoolset.org/>`_ is an open source
collection of tools used for building Windows installers (``.msi``
files, ``.exe``, etc). The WiX Toolset is incredibly powerful and
enables building anything from simple to complex installers.

Tugger defines a high-level interface to the WiX Toolset via Rust
APIs and exposes this functionality to Starlark.

.. _tugger_wix_concepts:

Concepts
========

With the WiX Toolset, you define your installer through ``.wxs`` XML
files. You use the ``candle.exe`` program to *compile* these files into
``.wixobj`` files. These *compiled* files are then *linked* together
using ``light.exe`` to produce an installer (``.msi``, ``.exe``, etc).

The goal of Tugger's Rust API is to expose the low-level control over
WiX Toolset that the most demanding applications will need while also
providing high-level and simpler interfaces for performing common tasks
(such as producing a simple ``.msi`` installer that simply materializes
files into the ``Program Files`` directory).

.. _tugger_wix_invoking:

How Tugger Invokes WiX
======================

Tugger's Rust APIs collects which ``.wxs`` files to compile and their
compilation settings. It also collects additional files needed to
compile ``.wxs`` files.

When you *build* your installer, Tugger copies all the registered ``.wxs``
files plus other registered files into a common directory. It then invokes
``candle.exe`` on each ``.wxs`` file followed by ``light.exe`` to link
them together. This is different from a traditional environment,
where ``.wxs`` files are often processed in place: Tugger always makes
copies to try to ensure results are reproducible and the full build
environment is captured.

.. _tugger_wix_predefined_templates:

Predefined Installer Templates
==============================

Tugger contains some pre-defined *templates* for common installer
functionality and APIs to populate them. These *templates* include:

* A simple MSI installer which will materialize a set of files in the
  ``Program Files`` directory.
* A bundle ``.exe`` installer which supports chaining multiple installers
  and automatically installing the Visual C++ Redistributable.

.. _tugger_wix_files_fragments:

Automatic ``<Fragment>`` Generation for Files
=============================================

Tugger supports automatically generating a ``.wxs`` file with
``<Fragment>``s describing a set of files. Given a set of input files,
it will produce a deterministic ``.wxs`` file with ``<DirectoryRef>``
holding ``<Component>`` and ``<File>`` of every file therein as well
as ``<ComponentGroup>`` for each distinct directory tree.

This functionality is similar to what WiX Toolset's ``heat.exe`` tool
can do. However, Tugger uses a deterministic mechanism to derive GUIDs
and IDs for each item. This enables the produced elements to be
referenced in other ``.wxs`` files more easily. And the generated file
doesn't need to be saved or manually updated, as it does with the use
of ``heat.exe``.

You simply give Tugger a manifest of files to index and the prefix
for ``Id`` attributes in XML, and it will emit a deterministic ``.wxs``
file!
