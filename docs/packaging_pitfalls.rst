.. _pitfalls:

==================
Packaging Pitfalls
==================

While PyOxidizer is capable of building fully self-contained binaries
containing a Python application, many Python packages and applications make
assumptions that don't hold inside PyOxidizer. This section talks about
all the things that can go wrong when attempting to package a Python
application.

.. _pitfall_extension_modules:

C and Other Native Extension Modules
====================================

Many Python packages compile *extension modules* to native code. (Typically
C is used to implement extension modules.)

PyOxidizer has varying levels of support for Python extension modules.
In many cases, everything *just works*. But there are known incompatibilities
and corner cases. See :ref:`packaging_extension_modules` for details.

Identifying PyOxidizer
======================

Python code may want to know whether it is running in the context of
PyOxidizer.

At packaging time, ``pip`` and ``setup.py`` invocations made by PyOxidizer
should set a ``PYOXIDIZER=1`` environment variable. ``setup.py`` scripts,
etc can look for this environment variable to determine if they are being
packaged by PyOxidizer.

At run-time, PyOxidizer will always set a ``sys.oxidized`` attribute with
value ``True``. So, Python code can test whether it is running in PyOxidizer
like so::

   import sys

   if getattr(sys, 'oxidized', False):
       print('running in PyOxidizer!')
