.. _config_files:

===================
Configuration Files
===================

PyOxidizer uses `Starlark <https://github.com/bazelbuild/starlark>`_
files to configure run-time behavior.

Starlark is a dialect of Python intended to be used as a configuration
language and the syntax should be familiar to any Python programmer.

Finding Configuration Files
===========================

The Starlark configuration file is processed as part of building the ``pyembed``
crate. This is the crate that manages an embedded Python interpreter in a
larger Rust project.

If the ``PYOXIDIZER_CONFIG`` environment variable is set, the path specified
by this environment variable will be used as the location of the Starlark
configuration file.

If ``PYOXIDIZER_CONFIG`` is not set, the build will look for a
``pyoxidizer.bzl`` file starting in the directory of the ``pyembed``
crate and then traversing ancestor directories until a file is found.

If no configuration file is found, an error occurs.

File Processing Semantics
=========================

A configuration file is evaluated in a custom Starlark *dialect* which
provides primitives used by PyOxidizer. This dialect provides some
well-defined global variables (defined in UPPERCASE) as well as some
types and functions that can be constructed and called. (See their
definitions below.)

A configuration file is effectively a sandboxed Python script. As
functions are called, PyOxidizer will perform actions as described
by those functions.

Configuration files define functions which perform some activity
then register these functions under a *target* name via the
``register_target()`` global function. When a configuration
file is evaluated, PyOxidizer attempts to resolve an ordered set of
*targets*. This means that configuration files are effectively a mini
build system, albeit without the complexity and features that a fully
generic build system entails.

Global Environment
==================

The evaluation context takes place in a *global environment*.

This environment contains
`built-in symbols and constants from Starlark <https://github.com/bazelbuild/starlark/blob/master/spec.md#built-in-constants-and-functions>`_
in addition to symbols and constants provided by ``PyOxidizer``. See
:ref:`config_api` for documentation of every available symbol and
type in our Starlark dialect.
