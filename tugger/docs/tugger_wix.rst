.. py:currentmodule:: starlark_tugger

.. _tugger_wix:

===================================================
Using the WiX Toolset to Produce Windows Installers
===================================================

The `WiX Toolset <https://wixtoolset.org/>`_ is an open source
collection of tools used for building Windows installers (``.msi``
files, ``.exe``, etc). The WiX Toolset is incredibly powerful and
enables building anything from simple to complex installers.

Tugger defines interfaces to the WiX Toolset via Rust APIs and exposes
much of this functionality to Starlark.

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

.. _tugger_wix_apis:

Tugger's WiX APIs
=================

Tugger implements various interfaces for interacting with WiX. This section
attempts to document them at a high level and talks about when to use
which.

``WxsBuilder``
   The ``WxsBuilder`` Rust struct is used to build a single ``.wxs`` file. You
   provide the path of the ``.wxs`` and build settings and it knows how to
   invoke ``candle.exe`` for this file.

``WiXInstallerBuilder``
   The ``WiXInstallerBuilder`` Rust struct and :py:class:`WiXInstaller`
   Starlark type are used to manage the end-to-end building and linking of
   ``.wxs`` files. This type knows how to register multiple ``WxsBuilder``
   instances and build them as a collection. This type holds all the logic
   for invoking ``candle.exe`` and ``light.exe``.

``WiXSimpleMSIBuilder``
   The ``WiXSimpleMSIBuilder`` Rust struct and :py:class:`WiXMSIBuilder`
   Starlark type provide a high-level interface for generating an MSI based
   installer with common features. It enables you to generate a ``.wxs`` file by
   providing a few parameters, without having to know WiX XML.

   A ``WiXSimpleMSIBuilder`` ultimately is converted to a ``WiXInstallerBuilder``.

``WiXBundleInstallerBuilder``
   The ``WiXBundleInstallerBuilder`` Rust struct and :py:class:`WiXBundleBuilder`
   Starlark type provide a high-level interface for generating an ``.exe``
   based installed with common features.

   A ``WiXBundleInstallerBuilder`` ultimately is converted to a
   ``WiXInstallerBuilder``.

If your application only needs the limited functionality exposed by the
high-level ``WiXSimpleMSIBuilder`` and ``WiXBundleInstallerBuilder`` interfaces,
you are encouraged to use these for building your installer, as you won't need
to concern yourself with the low-level WiX XML details.

If your application needs what you think is simple or common functionality
not provided by the aforementioned high-level builders, consider filing a
feature request to request the missing functionality.

Complex applications that have outgrown the limited capabilities of the
high-level *builder* interfaces will need to use the lower level
``WiXInstallerBuilder`` / :py:class:`WiXInstaller` interface.
This interface allows you to provide your own ``.wxs`` files. This means
you can still use Tugger for invoking WiX, even if all of your ``.wxs`` files
are maintained outside of Tugger, enabling Tugger to grow with your needs.
Note that it is possible to use one of the higher-level interfaces for
automatically generating a ``.wxs`` file and then supplement this
automatically-generated file with other ``.wxs`` files that you maintain.

.. note::

   Ideally no WiX installer should be too complicated to be handled by
   Tugger. If Tugger's functionality is not sufficient, consider
   `creating an issue <https://github.com/indygreg/PyOxidizer/issues/new>`_
   to request a feature to close the feature gap.

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

.. _tugger_wix_files_fragments:

Automatic ``<Fragment>`` Generation for Files
=============================================

Tugger supports automatically generating a ``.wxs`` file with
``<Fragment>``'s describing a set of files. Given a set of input files,
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
