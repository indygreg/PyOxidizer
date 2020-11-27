.. _tugger:

Shipping Applications with ``tugger``
=====================================

The Tugger project aims to make it easy to ship applications. It does so
by implementing generic functionality related to application distribution
in a myriad (fleet?) of individual, domain-specific crates. See
:ref:`tugger_crates` for more. Tugger supports generating distributable
artifacts in common formats such as Windows ``.msi`` installers, Debian
``.deb`` files, and Snapcraft ``.snap`` files.

Tugger's Rust crates can be consumed as regular Rust library crates by
any project and are explicitly designed for this use case. Tugger also
defines a Starlark dialect (Starlark is a Python-like configuration language),
enabling applications to define packaging functionality in configuration
files, which Tugger can execute. The Starlark dialect is effectively
a scriptable interface to Tugger's Rust internals.

Tugger is part of the PyOxidizer Project and is developed inside the
PyOxidizer repository at https://github.com/indygreg/PyOxidizer. However,
Tugger is designed to be a standalone project and doesn't require PyOxidizer.

.. toctree::
   :maxdepth: 2

   tugger_overview
   tugger_starlark
   tugger_wix
   tugger_history
