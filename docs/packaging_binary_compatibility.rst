.. _packaging_binary_compatibility:

=============================================
Portability of Binaries Built with PyOxidizer
=============================================

Binary portability refers to the property that a binary built in
machine/environment *X* is able to run on machine/environment *Y*.
In other words, you've achieved binary portability if you are able
to copy a binary to another machine and run it without modifications.

It is exceptionally difficult to achieve high levels of binary
portability for various reasons.

PyOxidizer is capable of building binaries that are highly *portable*.
However, the steps for doing so can be nuanced and vary substantially
by operating system and target platform. This document attempts to
capture the various steps and caveats involved.

.. important::

   Please create issues at https://github.com/indygreg/PyOxidizer/issues
   when documentation on this page is inaccurate or lacks critical
   details.

Using ``pyoxidizer analyze`` For Assessing Binary Portability
=============================================================

The ``pyoxidizer analyze`` command can be used to analyze the contents
of executables and libraries. It can be used as a PyOxidizer-specific
tool for assessing the portability of built binaries.

For example, for ELF binaries (the binary format used on Linux), this
command will list all shared library dependencies and analyze glibc
symbol versions and print out which Linux distribution versions it
thinks the binary is compatible with.

.. note::

   ``pyoxidizer analyze`` is not yet feature complete on all platforms.

Python Distribution Versus Built Application Portability
========================================================

PyOxidizer ships with specially built Python distributions that are
highly portable. See :ref:`packaging_available_python_distributions`
for the full list of these distributions and
:ref:`packaging_python_distribution_portability` for details on the
portability of these Python distributions.

Generally speaking, you don't have to worry about the portability
of the Python distributions because the distributions tend to
*just work*.

.. important::

   The machine and environment you use to run ``pyoxidizer`` has
   critical implications for the portability of built binaries.

When you use PyOxidizer to produce a new binary (an executable or
library), you are compiling *new* code and linking it in an environment
that is different from the specialized environment used to build the
built-in Python distributions. This means that the binary portability
of your built binary is effectively defined by the environment
``pyoxidizer`` was run from.

.. _packaging_windows_portability:

Windows Portability
===================

The built-in Python distributions have a run-time dependency on
various DLLs. All 3rd party DLLs (OpenSSL, SQLite3, etc) required
by Python extensions are provided by the built-in distributions.

Many DLL dependencies should be present in any Windows installation.

The Python distributions also have a dependency on the Visual Studio
C++ Runtime. You will need to distribute a copy of ``vcruntimeXXX.dll``
alongside your binary or trigger the install of the Visual Stdio
C++ Redistributable in your application installer so the dependency
is managed at the system level. (Installing the redistributable via
an installer is preferred per Microsoft documentation.)

There is also a dependency on the Universal C Runtime (UCRT). However,
the UCRT is a Windows operating system component and is always present
in installations of Windows 10 and Windows Server 2016 and newer. So
unless you are targeting Windows 8 or Windows Server 2012, you don't
need to worry about the UCRT.

PyOxidizer supports automatically materializing the runtime dependencies
via multiple mechanisms.

:ref:`config_python_executable_to_wix_bundle_builder` produces an ``.exe``
installer embedding a copy of the ``vc_redist<arch>.exe`` installer for
the target platform. At installer creation time, a deterministic and
SHA-256 verified copy of the ``vc_redist<arch>.exe`` files are downloaded
from Microsoft's servers and embedded in the installer. At install time,
the embedded installers are executed (if needed) and the VC++ files are
installed globally on the target system.

:ref:`config_type_python_executable_windows_runtime_dlls_mode` controls
whether the build process for executables attempts to copy runtime DLLs
next to the built executable. Since Windows attempts to load DLLs next to
the executable, if the DLLs are present, this should *just work*.

:ref:`config_type_python_executable_windows_runtime_dlls_mode` treats the
DLL dependencies as additional extra files to be materialized with the
built binary. So the additional files will be there for local builds
(e.g. ``pyoxidizer build`` or ``pyoxidizer run``) as well as be
materialized in file lists when using methods like
:ref:`config_python_executable_to_file_manifest` or
:ref:`config_python_executable_to_wix_msi_builder`.

Unlike the ``vc_redist<arch>.exe`` mechanism (which obtains an official
Microsoft installer from the Internet), the *local DLL files* mode relies
on locating DLLs on the local system. Specifically, ``vswhere.exe`` is
located and used to find packages/files in a Visual Studio installation.
So if ``vswhere.exe``, Visual Studio, or the Visual Studio components
providing these files isn't installed, this operation can fail. Generally,
installing a modern version of Visual Studio with support for building
C/C++ applications is sufficient. The specific component required is
``Microsoft.VisualCPP.Redist.<version>.Latest``, where ``<version>`` is
likely ``14``.

PyOxidizer does not currently support automatically materializing the
Universal C Runtime (UCRT) files. See
https://docs.microsoft.com/en-us/cpp/windows/universal-crt-deployment for
instructions on deploying the UCRT with your application.

PyOxidizer does also not yet support sniffing binaries for missing DLL
dependencies. So you may need to manually add additional DLLs to your
application install layout to satisfy run-time dependencies.

For more information, see the
`Microsoft documentation <https://docs.microsoft.com/en-us/cpp/windows/deploying-native-desktop-applications-visual-cpp?view=vs-2019>`_
on deployment considerations for Windows binaries.

The `Dependency Walker <http://www.dependencywalker.com/>`_ tool is also
useful for analyzing DLL dependencies.

Windows binaries tend to be highly portable by default. If you follow
Microsoft's guidelines and install all required DLLs, you should be
set.

macOS Portability
=================

The built-in Python distributions are built with
``MACOSX_DEPLOYMENT_TARGET=10.9``, so they should be compatible with
macOS versions 10.9 and newer.

The Python distribution has dependencies against a handful of system
libraries and frameworks. These frameworks should be present on all
macOS installations.

From your build environment, you may want to also ensure
``MACOSX_DEPLOYMENT_TARGET`` is set to ensure references to newer
macOS SDK features aren't present.

Apple's `Xcode documentation <https://developer.apple.com/documentation/xcode>`_
has various guides useful for further consideration.

Linux Portability
=================

Linux is the most difficult platform to tackle for binary portability.
There's a strongly held attitude that binaries should be managed as
packages by the operating system and these packages are built in such
a way that the package manager handles all the details for you. If you
stray from the *paved road* and choose not to use the package manager
provided by your operating system with the package sources configured
by default, things get very challenging very quickly.

The best way to produce a portable Linux binary is to produce a
fully statically-linked binary. There are no shared libraries to
worry about and generally speaking these binaries *just work*. See
:ref:`statically_linked_linux` for more.

If you produce a dynamic binary with library dependencies, things are
complicated.

Nearly every binary built on Linux will require linking against ``libc``
and will require a symbol provided by ``glibc``. ``glibc`` versions
it symbols. And when the linker resolves those symbols at link time,
it usually uses the version of ``glibc`` being linked against. For
example, if you link on a machine with ``glibc`` 2.19, the symbol
versions in the produced binary will be against version 2.19 and
the binary will load against ``glibc`` versions >=2.19. But if
you link on a machine with ``glibc`` 2.29, symbol versions are against
version 2.29 and you can only load against versions >= 2.29.

This means that to ensure maximum portability, you want to link against
old ``glibc`` symbol versions. While it is possible to use old symbol
versions when a more modern ``glibc`` is present, the path of least
resistance is to build in an environment that has an older ``glibc``.

The built-in Linux distributions use Debian 8 (Jessie) as their build
environment. So a Debian 8 build environment is a good candidate
to build on. Ubuntu 14.04, OpenSUSE 13.2, OpenSUSE 42.1, RHEL/CentOS 7,
and Fedora 21 (``glibc`` 2.20) are also good candidates for build
environments.

Of course, if you are producing distribution-specific binaries and/or
control installation (so e.g. dependencies are installed automatically),
this matters less to you.

Again, the ``pyoxidizer analyze`` command can be very useful for
inspecting binaries for portability and alerting you to any potential
issues.
