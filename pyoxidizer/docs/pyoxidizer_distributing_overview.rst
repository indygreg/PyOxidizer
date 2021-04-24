.. py:currentmodule:: starlark_pyoxidizer

.. _pyoxidizer_distributing_overview:

========
Overview
========

Application *distribution* in PyOxidizer is fundamentally a separate domain
from *building* or *packaging* applications. One way to think about this
is *building* is concerned with producing files constituting your application -
the executables and support files needed at run-time - and *distribution* is
concerned with installing those files on other machines.

PyOxidizer uses the :ref:`Tugger <tugger>` tool to handle most *distribution*
functionality. Tugger is a Rust crate and Starlark dialect developed alongside
PyOxidizer that specializes in functionality required to *distribute* applications.
Tugger is technically a separate project. But PyOxidizer provides full access to
Tugger's Starlark functionality and even extends it to make distributing Python
applications simpler.

Using Tugger Starlark
=====================

Tugger defines a Starlark dialect that enables you to produce distributable
artifacts. See :ref:`tugger_starlark` for the documentation of this dialect.

The full Tugger Starlark dialect is available to PyOxidizer configuration files.

PyOxidizer configuration files have the option of using the generic Tugger
Starlark primitives and using supplemental/extended functionality provided by
PyOxidizer's Starlark dialect. The Tugger-provided primitives are generally
low-level and generic. The PyOxidizer-provided extensions are Python specific
and may allow simpler configuration files.

See other documentation in :ref:`pyoxidizer_distributing` for details on
PyOxidizer's extensions to Tugger's Starlark dialect and how to perform common
*distribution* actions.
