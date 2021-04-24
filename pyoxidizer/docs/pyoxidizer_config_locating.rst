.. py:currentmodule:: starlark_pyoxidizer

.. _config_locating:

================================
Automatic File Location Strategy
================================

If the ``PYOXIDIZER_CONFIG`` environment variable is set, the path specified
by this environment variable will be used as the location of the Starlark
configuration file.

If the ``OUT_DIR`` environment variable is set (we're building from the
context of a Rust project), the ancestor directories will be searched for
a ``pyoxidizer.bzl`` file and the first one found will be used.

Otherwise, ``PyOxidizer`` will look for a ``pyoxidizer.bzl`` file starting in
either the current working directory or from the directory containing the
``pyembed`` crate and then will traverse ancestor directories until a file is
found.

If no configuration file is found, an error occurs.
