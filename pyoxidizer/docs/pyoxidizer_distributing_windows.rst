.. _pyoxidizer_distributing_windows:

=======================================
Distribution Considerations for Windows
=======================================

This document describes some of the considerations when you want to
install/run a PyOxidizer-built application on a separate Windows machine
from the one that built it.

.. important::

   The restrictions in this document regard the run-time / target environment
   that a binary will run on: they do not describe the environment used to
   build that binary. In many cases, a binary built on Windows 10 or Windows
   Server 2019 will work fine on earlier operating system versions.

Readers may also find the
`Microsoft documentation <https://docs.microsoft.com/en-us/cpp/windows/deploying-native-desktop-applications-visual-cpp?view=vs-2019>`_
on deployment considerations for Windows binaries a useful resource to
supplement this document with more generic considerations.

.. _pyoxidizer_distributing_windows_os_requirements:

Operating System Requirements
=============================

The default :ref:`Python distributions <packaging_python_distributions>` used
by PyOxidizer require Windows 8 or Windows 2012 or newer.

The official Python 3.8 Windows distributions available on www.python.org
support Windows 7. PyOxidizer has chosen to drop support for Windows 7 to
simplify support.

In addition to the restrictions imposed by the Python distribution in use,
Rust may impose its own restrictions. However, Rust has historically produced
binaries that work on Windows 8 and Windows 2012, so this likely is not
an issue.

.. _pyoxidizer_distributing_windows_dll_requirements:

General Runtime / DLL Dependencies
==================================

The default :ref:`Python distributions <packaging_python_distributions>` used
by PyOxidizer require the Microsoft Visual C++ Redistributable and Universal
CRT (UCRT).

The ``standalone_dynamic`` distributions (the default distribution flavor) have
a run-time dependency on various 3rd party DLLs used by extensions (OpenSSL,
SQLite3, etc). However, these 3rd party DLLs are part of the Python distribution
and PyOxidizer should automatically install them if they are required.

All other DLL dependencies required by the default Python distributions should
be core Windows operating system components and always available, even in a
freshly installed Windows machine.

.. _pyoxidizer_distributing_windows_application_dependencies:

Application Specific Dependencies
=================================

When adding custom behavior to your application, PyOxidizer makes some
effort to ensure additional dependencies (beyond the operating system,
Python distribution, and Microsoft runtimes) are met. However, there are
limitations to this.

When installing custom Python packages, PyOxidizer attempts to identify and
install compiled Python extensions and ``.dll`` dependencies distributed
with that package. See :ref:`packaging_additional_files` for more. However,
there are corner cases and occasional bugs that may prevent this from working
correctly.

To ensure are DLL dependencies are properly captured, it is recommend to
inspect your binaries for references to missing DLLs before distributing
them. The `Dependency Walker <http://www.dependencywalker.com/>`_ tool can
be used for this. ``pyoxidizer analyze`` may also provide useful information.

In many cases, installing a missing DLL is a matter of installing the DLL
next to your application/binary by treating the DLL as an *additional file*
from the Starlark configuration. See :ref:`packaging_additional_files`
for more.

When possible, it is recommended to test your application in a freshly
installed Windows environment to ensure it works. Please note that many
Windows virtual machines already contain additional software and may not
reflect real world deployment targets.

.. _pyoxidizer_distributing_windows_vc_redist:

Managing the Visual C++ Redistributable Requirement
===================================================

Binaries built with PyOxidizer often have a run-time dependency on the
Microsoft Visual C++ Redistributable. These are DLLs with filenames like
``vcruntime140.dll`` and ``vcruntime140_1.dll``.

.. important::

   The Visual C++ Redistributable is **not** a core Windows operating system
   component and any distributed Windows application **must take measures to
   ensure the Visual C++ Redistributable is available on the remote machine**
   or the application may fail to run with a missing DLL error.

See Microsoft's
`Redistributing Visual C++ Files <https://docs.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-160>`_
documentation for the canonical source on distribution requirements.

PyOxidizer has built-in features to make satisfying these requirements turnkey.
Read the sections below for details of each.

.. _pyoxidizer_distributing_windows_vc_redist_installer:

Installing the Visual C++ Redistributable as Part of Your Application Installer
-------------------------------------------------------------------------------

PyOxidizer can produce Windows ``.exe`` application installers that embed a
copy of the Microsoft Visual C++ Redistributable installer (files named
``vc_redist<arch>.exe``) and automatically run this installer during application
install.

The way this works is PyOxidizer contains a reference to the URL and SHA-256
of these ``vc_redist<arch>.exe`` installers. When your application installer is
built, these files are downloaded from Microsoft's servers and embedded in the
new meta-installer. At install time, these embedded installers are executed
automatically (if they need to be) and the Visual C++ files are installed at
the system level, where they are available to any application.

If a newer version of the Visual C++ Redistributable files are already present,
the installer should no-op instead of downgrading what's already installed.

The following Starlark functionality can be used to bundle the
Visual C++ Redistributable installer as part of your application installer:

* :ref:`config_python_executable_to_wix_bundle_builder`
* :ref:`tugger_starlark_type_wix_bundle_builder.add_vc_redistributable`

.. _pyoxidizer_distributing_windows_vc_redist_local:

Installing the Visual C++ Redistributable Files Locally Next to Your Binary
----------------------------------------------------------------------------

Another method of installing the Visual C++ Redistributable files is to
distribute copies of the DLLs next to the binary that loads them. e.g. if
you produce a ``myapp.exe``, there will be a ``vcruntime140[_1].dll`` in the
same directory as ``myapp.exe``. Since Windows attempts to load DLLs next to
the executable, if the DLLs are present, this should *just work*.

PyOxidizer supports automatically finding and copying the required DLLs
in this manner. The Starlark setting controlling this behavior is
:ref:`config_type_python_executable_windows_runtime_dlls_mode`.

This setting effectively instructs the ``PythonExecutable`` building code
to materialize extra files next to the binary. The Visual C++ files are
treated just like any other supplementary files (like Python resources).
This means that Visual C++ files will be materialized on the filesystem when
running ``pyoxidizer build``, ``pyoxidizer run``. The files will also
be present in file lists when using Starlark methods like
:ref:`config_python_executable_to_file_manifest` or
:ref:`config_python_executable_to_wix_msi_builder`.

This *local files* mode relies on locating DLLs on the local system. It does
so using ``vswhere.exe`` to locate a Visual Studio installation containing
the ``Microsoft.VisualCPP.Redist.<version>.Latest`` component (``<version>``
is ``14`` for ``vcruntime140.dll``). This should *just work* if you have
Visual Studio 2017 or 2019 installed with support for building C/C++
applications. If the files cannot be found, run the Visual Studio Installer,
``Modify`` your installation, go to ``Individual Components``, search for
``redistributable``, and make sure all items are checked.

.. important::

   It is possible to include a copy of the Visual C++ Redistributable in
   both your application installer and as files local to the built binary.
   This behavior is redundant and will likely result in the local files
   being used.

   When including the Visual C++ Redistributable installer as part of your
   deployment solution, it is recommended to set
   ``PythonExecutable.windows_runtime_dlls_mode = "never"`` to prevent them
   from being redundantly installed.

.. _pyoxidizer_distributing_windows_ucrt:

Managing the Universal CRT (UCRT) Requirement
==============================================

Binaries built with PyOxidizer may have a run-time dependency on the
Universal C Runtime (UCRT).

The UCRT is a Windows operating system component and is always present in
installations of Windows 10, Windows Server 2016, and newer. Combined with
PyOxidizer's Windows version requirements, this means you don't need to
worry about the UCRT unless you are targeting Windows 8 or Windows Server 2012.

PyOxidizer does not currently support automatically materializing the
UCRT. See
https://docs.microsoft.com/en-us/cpp/windows/universal-crt-deployment for
instructions on deploying the UCRT with your application.

We are receptive to adding a feature to support more turnkey UCRT
management if there is interest in it.
