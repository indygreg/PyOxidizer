.. _pyoxidizer_distributing_wix:

================================================
Building Windows Installers with the WiX Toolset
================================================

PyOxidizer supports building Windows installers (e.g. ``.msi`` and ``.exe``
installer files) using the `WiX Toolset <https://wixtoolset.org/>`_.
PyOxidizer leverages the :ref:`Tugger shipping tool <tugger>` for
integrating with WiX. See :ref:`tugger_wix` for the full Tugger WiX
documentation.

Tugger - and PyOxidizer by extension - are able to automatically create
XML files used by WiX to define installers with common features as well
as use pre-existing WiX files. This enables Tugger/PyOxidizer to facilitate
both simple and arbitrarily complex use cases.

Extensions to Tugger Starlark Dialect
=====================================

PyOxidizer supplements Tugger's Starlark dialect with additional
functionality that makes building Python application installers simpler. For
example, instead of manually constructing a WiX installer, you can call
a method on a Python Starlark type to convert it into an installer.

PyOxidizer provides the following extensions and integrations with
:ref:`Tugger's Starlark dialect <tugger_starlark>`:

:ref:`config_type_file_manifest.add_python_resource`
   Adds a Python resource type to Tugger's
   :ref:`tugger_starlark_type_file_manifest` type.

:ref:`config_type_file_manifest.add_python_resources`
   Adds an iterable of Python resource types to Tugger's
   :ref:`tugger_starlark_type_file_manifest` type.

:ref:`config_python_executable_to_file_manifest`
   Converts a :ref:`config_type_python_executable` to a
   :ref:`tugger_starlark_type_file_manifest`. Enables materializing an
   executable/application as a set of files, which Tugger can easily operate
   against.

:ref:`config_python_executable_to_wix_bundle_builder`
   Converts a :ref:`config_type_python_executable` to a
   :ref:`tugger_starlark_type_wix_bundle_builder`.

   This method will produce a :ref:`tugger_starlark_type_wix_bundle_builder`
   that is pre-configured with appropriate settings and state for a Python
   application. The produced ``.exe`` installer should *just work*.

:ref:`config_python_executable_to_wix_msi_builder`
   Converts a :ref:`config_type_python_executable` to a
   :ref:`tugger_starlark_type_wix_msi_builder`.

   This method will produce a :ref:`tugger_starlark_type_wix_msi_builder`
   that is pre-configured to install a Python application and all its
   support files. The MSI will install all files composing the Python
   application, excluding system-level dependencies.

.. _pyoxidizer_distributing_wix_choosing:

Choosing an Installer Creation Method
=====================================

Tugger provides multiple Starlark primitives for defining Windows installers
built with the WiX Toolset. Which one should you use?

See :ref:`tugger_wix_apis` for a generic overview of this topic. The
remainder of this documentation will be specific to Python applications.

It is is important to call out that unless you are using the *static*
:ref:`Python distributions <packaging_python_distributions>`, binaries built
with PyOxidizer will have a run-time dependency on the Visual C++
Redistributable runtime DLLs (e.g. ``vcruntime140.dll``). Many Windows
applications have a dependency on these DLLs and most Windows machines have
installed an application that has installed the required DLLs. So not
distributing ``vcruntimeXXX.dll`` with your application may *just work*
most of the time. However, on a fresh Windows installation, these required
files may not exist. So it is important that they be installed with your
application.

When using :ref:`config_python_executable_to_wix_msi_builder` or
:ref:`config_python_executable_to_wix_bundle_builder`, PyOxidizer
will automatically add the Visual C++ Redistributable to the installer
if it is required. However, the method varies. For bundle installers,
the installer will contain the official ``VC_Redist*.exe`` installer
and this installer will be executed as part of running your application's
installer. For MSI installers, Tugger will attempt to locate the
``vcruntimeXXX.dll`` files on your system (this requires an
installation of Visual Studio) and copy these files next to your
built/installed executable.

If you are not using one of the aforementioned APIs to create your
installer, you will need to explicitly add the Visual C++ Redistributable
to your installer.
The :ref:`tugger_starlark_type_wix_msi_builder.add_visual_cpp_redistributable`
and :ref:`tugger_starlark_type_wix_bundle_builder.add_vc_redistributable`
Starlark methods can be called to do this. (PyOxidizer's Starlark methods
for creating WiX installers effectively call these methods.)
