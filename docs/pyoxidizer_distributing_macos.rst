.. _pyoxidizer_distributing_macos:

=====================================
Distribution Considerations for macOS
=====================================

This document describes some of the considerations when you want to
install/run a PyOxidizer-built application on a separate macOS machine
from the one that built it.

.. _pyoxidizer_distributing_macos_os_requirements:

Operating System and Architecture Requirements
==============================================

PyOxidizer has support for targeting x86_64 (Intel) and aarch64 (ARM)
Apple devices. The default
:ref:`Python distributions <packaging_python_distributions>` target
macOS 10.9+ for x86_64 and 11.0+ for aarch64.

.. _pyoxidizer_distributing_macos_python_distributions:

Python Distribution Dependencies
================================

The default :ref:`Python distributions <packaging_python_distributions>` used
by PyOxidizer have dependencies on system libraries outside of the Python
distribution.

The `python-build-standalone project <https://python-build-standalone.readthedocs.io/en/latest/>`_
has gone to great lengths to ensure that the Python distributions only link
against external libraries and symbols that are present on a default macOS
installation.

The default Python distributions are built to target macOS 10.9 on x86_64 and
11.0 on aarch64. So they should *just work* on those and any newer versions
of macOS.

.. _pyoxidizer_distributing_macos_single_arch:

Single Architecture Binaries
============================

PyOxidizer currently only emits single architecture binaries.

Multiple architecture binaries (often referred to as *universal* or *fat*
binaries) can not (yet) be emitted natively by PyOxidizer.

This means that if you distribute a binary produced by PyOxidizer and want it
to run on both Intel and ARM machines, you will need to maintain separate
artifacts for Intel and ARM machines or you will need to produce a *fat* binary
outside of PyOxidizer.

https://github.com/indygreg/PyOxidizer/issues/372 tracks implementing
support for emitting *fat* binaries from PyOxidizer. Please engage there
if this feature is important to you.

.. _pyoxidizer_distributing_macos_managing_portability:

Managing Portability of Built Applications
==========================================

Like Linux, the macOS build environment can *leak* into the built
application and introduce additional dependencies and degrade the portability
of the default Python distributions.

It is common for built binaries to pull in modern macOS SDK features. A
common way to prevent this is to set the ``MACOS_DEPLOYMENT_TARGET``
environment variable during the build to the oldest version of macOS you
want to support. The default Python distributions target macOS 10.9, so to
set the same compatibility level, do something like this::

   $ MACOSX_DEPLOYMENT_TARGET=10.9 pyoxidizer build

Apple's `Xcode documentation <https://developer.apple.com/documentation/xcode>`_
has various guides useful for further consideration.
