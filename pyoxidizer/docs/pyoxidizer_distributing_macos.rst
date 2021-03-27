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

.. _pyoxidizer_distributing_macos_build_machine_requirements:

Build Machine Requirements
==========================

PyOxidizer needs to link new binaries containing Python. Due to the way
linking works on Apple platforms, you **must** use an Apple SDK no older
than the one used to build the Python distributions or linker errors
(likely undefined symbols) can occur.

PyOxidizer will automatically attempt to locate, validate, and use an
appropriate Apple SDK given requirements specified by the Python distribution
in use. If you have Xcode or the Xcode Commandline Tools installed,
PyOxidizer should be able to locate Apple SDKs automatically. When building,
PyOxidizer will print information about Apple SDK discovery. More details
are printed when running ``pyoxidizer --verbose``.

PyOxidizer will automatically look for SDKs in the directory specified
by ``xcode-select --print-path``. This path is often
``/Applications/Xcode.app/Contents/Developer``. You can specify an alternative
directory by setting the ``DEVELOPER_DIR`` environment variable. e.g.::

   DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer pyoxidizer build

You can override PyOxidizer's automatic SDK discovery by setting ``SDKROOT``
to the base directory of an Apple SDK you want to use. (If you find yourself
doing this to work around SDK discovery *bugs*, please consider creating a
GitHub issue to track the problem.) e.g.::

   SDKROOT=/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk pyoxidizer build

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
common way to prevent this is to set the ``MACOSX_DEPLOYMENT_TARGET``
environment variable during the build to the oldest version of macOS you
want to support.

The default :ref:`Python distributions <packaging_python_distributions>` target
macOS 10.9 on x86_64 and 11.0 on aarch64.

.. important::

   PyOxidizer will automatically set the deployment target to match what the
   Python distribution was built with, so in many cases you don't need to
   worry about version targeting.

If you wish to override the default deployment targets, set an alternative
value using the appropriate environment variable.::

   $ MACOSX_DEPLOYMENT_TARGET=10.15 pyoxidizer build

Apple's `Xcode documentation <https://developer.apple.com/documentation/xcode>`_
has various guides useful for further consideration.
