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

.. _packaging_windows_portability:

Windows Portability
===================

See :ref:`pyoxidizer_distributing_windows`.

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

See :ref:`pyoxidizer_distributing_linux`.
