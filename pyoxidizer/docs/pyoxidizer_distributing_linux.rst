.. py:currentmodule:: starlark_pyoxidizer

.. _pyoxidizer_distributing_linux:

=====================================
Distribution Considerations for Linux
=====================================

This document describes some of the considerations when you want to
install/run a PyOxidizer-built application on a separate Linux machine
from the one that built it.

.. _pyoxidizer_distributing_linux_musl:

Exception for musl libc Binaries
================================

Linux binaries built against musl libc (e.g. the ``x86_64-unknown-linux-musl``
target triple) generally work on any Linux machine supporting the target
architecture. This is because musl libc linked binaries are fully
statically linked and therefore self-contained.

If you run ``ldd /path/to/binary`` and it prints ``not a dynamic
executable``, that binary is likely highly portable.

See :ref:`statically_linked_linux` for instructions on building binaries
with musl libc.

The rest of this document likely doesn't apply if using musl libc.

.. _pyoxidizer_distributing_linux_python_distribution_dependencies:

Python Distribution Dependencies
================================

The default :ref:`Python distributions <packaging_python_distributions>` used
by PyOxidizer have dependencies on shared libraries outside of the Python
distribution.

However, the
`python-build-standalone project <https://python-build-standalone.readthedocs.io/en/latest/>`_ -
the entity building the default Python distributions - has gone to great lengths
to ensure that all dependencies are common to nearly every Linux system and that
the Python distribution binaries should be highly portable across machines.

The ``*-unknown-linux-gnu`` builds have a dependency against GNU libc (glibc),
specifically ``libc.so.6``. However, the python-build-standalone project has
build-time validation that glibc version numbers in referenced symbols aren't
higher than glibc 19 (released in 2014). This should make binaries compatible
with the following common distributions:

* Fedora 21+
* RHEL/CentOS 7+
* openSUSE 13.2+
* Debian 8+ (Jessie)
* Ubuntu 14.04+

In addition to glibc, Python distributions also link to a handful of other
system libraries. Most of the libraries are part of the
`Linux Standard Base <https://refspecs.linuxfoundation.org/lsb.shtml>`_
specification and should be present on any conforming Linux distribution.

Some shared library dependencies are only pulled in by single Python
extensions. For example, ``libcrypto.so.1`` is likely only needed by the
``crypt`` extension. Distributors wanting to minimize the number of shared
library dependencies can do so by pruning Python extensions from the
install set. The ``PYTHON.json`` file in the extracted Python distribution
archive can be used to inspect which libraries are required by which
extensions.

.. _pyoxidizer_distributing_linux_built_app_dependencies:

Built Application Dependencies
==============================

While the default Python distributions used by PyOxidizer are highly
portable, the same cannot be said for binaries built with PyOxidizer.

.. important::

   The machine and environment you use to run ``pyoxidizer`` has critical
   implications for the portability of built binaries.

When you use PyOxidizer to produce a new binary (an executable or
library), you are compiling *new* code and linking it in an environment
that is different from the specialized environment used to build the
default Python distributions. This often means that the binary portability
of your built binary is effectively defined by the environment
``pyoxidizer`` was run from.

As a concrete example, if you run ``pyoxidizer build`` on an Ubuntu 20.10
machine and then ``pyoxidizer analyze`` the resulting ELF binary, you'll
find that it has a dependency on ``libgcc_s.so.1`` and it references glibc
2.32 symbol versions. This despite the default Python distribution not
depending on `libgcc_s.so.1`` and only glibc version 2.19.

What's happening here is the compiler/build settings from the building
machine are *leaking* into new binaries, likely as part of compiling
Rust code.

.. _pyoxidizer_distributing_linux_managing_portability:

Managing Binary Portability on Linux
====================================

Linux is a difficult platform to tackle for binary portability.

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

A similar story plays out with a dependency on ``libgcc_s.so.1``.

The default Python distributions use Debian 8 (Jessie) as their build
environment. So a Debian 8 build environment is a good candidate
to build on. Ubuntu 14.04, OpenSUSE 13.2, OpenSUSE 42.1, RHEL/CentOS 7,
and Fedora 21 (``glibc`` 2.20) are also good candidates for build
environments.

Of course, if you are producing distribution-specific binaries and/or
control installation (so e.g. dependencies are installed automatically),
this matters less to you.

The ``pyoxidizer analyze`` command can be very useful for inspecting
binaries for portability and alerting you to any potential issues.
