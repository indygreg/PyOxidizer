.. _pyoxidizer_distributing_wix:

================================================
Building Windows Installers with the WiX Toolset
================================================

Tugger supports building Windows installers (e.g. ``.msi`` and ``.exe`` files)
with the `WiX Toolset <https://wixtoolset.org/>`_. See :ref:`tugger_wix` for
the full Tugger documentation of this feature.

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

PyOxidizer provides the following extensions and integrations with Tugger's
Starlark dialect:

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

First, it is is important to call out that unless you are using the
*static* :ref:`Python distributions <packaging_python_distributions>`,
binaries built with PyOxidizer will have a run-time dependency on the
Visual C++ runtime (e.g. ``vcruntime140.dll``). PyOxidizer does
not explicitly distribute a ``vcruntimeXXX.dll`` file next to your binary
by default. The MSI installers will not contain a ``vcruntimeXXX.dll``
unless you explicitly add one in your Starlark configuration file!

To install the Visual C++ Redistributable/Runtime, it is recommended to
use *bundle installer* support in WiX to produce an ``.exe`` installer
which contains the Visual C++ Redistributable installer as well as your
application's MSI installer. **This is the most reliably way to install
the Visual C++ Runtime dependency**.
:ref:`config_python_executable_to_wix_bundle_builder` will install the
Visual C++ Redistributable by default and Tugger's
:ref:`tugger_starlark_type_wix_bundle_builder.add_vc_redistributable` can
be called to add the Visual C++ Redistributable to bundle installers
created via Tugger's Starlark primitives.

Many Windows applications have a dependency on the Visual C++ Runtime
and most Windows machines have installed an application that has installed
the required DLLs. So forgoing the explicit inclusion of the Visual C++
Redistributable from installers may *just work* 99% of the time. However,
on a fresh Windows installation, these required files may not exist, so
it is recommended to install the Visual C++ Redistributable as part of
your installer to ensure all dependencies are present.

Tugger supports generating WiX XML files from scratch, with minimal
configuration. This enables you to produce ``.msi`` or ``.exe`` installers
without having to concern yourself with WiX XML implementation details.

If your application only needs the limited WiX functionality provided
by these *builder* interfaces in Tugger (
:ref:`tugger_starlark_type_wix_bundle_builder` and
:ref:`tugger_starlark_type_wix_msi_builder`), you are highly encouraged to use
these interfaces for constructing your WiX-based installer.

Complex applications may outgrow the limited capabilities of the *builder*
interfaces exposed by Tugger. If this occurs, the
:ref:`tugger_starlark_type_wix_installer` type can be used to consume
existing ``.wxs`` XML files. This means that you can still use Tugger for
invoking WiX, even as your WiX installer becomes more complicated and you
need to maintain your own custom WiX XML files.

.. note::

   Ideally no WiX installer should be too complicated to be handled by
   Tugger. If Tugger's functionality is not sufficient, consider
   `creating an issue <https://github.com/indygreg/PyOxidizer/issues/new>`_
   to request a feature to close the feature gap.
