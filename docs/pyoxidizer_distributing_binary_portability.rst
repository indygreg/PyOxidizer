.. _pyoxidizer_distributing_binary_portability:

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
by operating system and target platform.

This document outlines some general strategies for tackling binary
portability. Please also consult the various platform-specific
documentation on this topic:

* :ref:`pyoxidizer_distributing_linux`
* :ref:`pyoxidizer_distributing_macos`
* :ref:`pyoxidizer_distributing_windows`

.. important::

   Please create issues at https://github.com/indygreg/PyOxidizer/issues
   when documentation on this subject is inaccurate or lacks critical
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
