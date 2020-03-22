.. _packaging_binary_compatibility:

====================
Binary Compatibility
====================

Binaries produced with PyOxidizer should be able to run nearly anywhere.
The details and caveats vary depending on the operating system and target
platform and are documented in the sections below.

.. important::

   Please create issues at https://github.com/indygreg/PyOxidizer/issues
   when the content of this section is incomplete or lacks important
   details.

The ``pyoxidizer analyze`` command can be used to analyze the contents
of executables and libraries. For example, for ELF binaries it will list
all shared library dependencies and analyze glibc symbol versions and
print out which Linux distributions it thinks the binary is compatible
with. Please note that ``pyoxidizer analyze`` is not yet implemented on
all platforms.

Windows
=======

Binaries built with PyOxidizer have a run-time dependency on various
DLLs. Most of the DLLs are Windows system DLLs and should always be
installed.

Binaries built with PyOxidizer have a dependency on the Visual Studio
C++ Runtime. You will need to distribute a copy of ``vcruntimeXXX.dll``
alongside your binary or trigger the install of the Visual Studio
C++ Redistributable in your application installer so the dependency is
managed at the system level (the latter is preferred).

There is also currently a dependency on the Universal C Runtime (UCRT).

PyOxidizer will eventually make producing Windows installers from packaged
applications turnkey. Until that time arrives, see the
`Microsoft documentation <https://docs.microsoft.com/en-us/cpp/windows/deploying-native-desktop-applications-visual-cpp?view=vs-2019>`_
on deployment considerations for Windows binaries. The
`Dependency Walker <http://www.dependencywalker.com/>`_ tool is also
useful for analyzing DLL dependencies.

macOS
=====

The Python distributions that PyOxidizer consumers are built with
``MACOSX_DEPLOYMENT_TARGET=10.9``, so they should be compatible with
macOS versions 10.9 and newer.

The Python distribution has dependencies against a handful of system
libraries and frameworks. These frameworks should be present on all
macOS installations.

Linux
=====

On Linux, a binary built with musl libc should *just work* on pretty much
any Linux machine. See :ref:`statically_linked_linux` for more.

If you are linking against ``libc.so``, things get more complicated
because the binary will probably link against the ``glibc`` symbol versions
that were present on the build machine. To ensure maximum binary
compatibility, compile your binary in a Debian 7 or 8 environment, as this
will use a sufficiently old version of glibc which should work in most
Linux environments.

Of course, if you control the execution environment (like if executables
will run on the same machine that built them), then this may not pose a
problem to you. Use the ``pyoxidizer analyze`` command to inspect binaries
for compatibility before distributing a binary so you know what the
requirements are.
