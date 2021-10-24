.. _pyembed_building:

========
Building
========

A design goal of ``pyembed`` is for it to exist like normal Rust
crates. However, because ``pyembed`` needs to link against Python,
there are some special requirements.

Configuring PyO3
================

``pyembed`` pulls in a Python library link dependency via the ``pyo3``
crate. At ``cargo build`` time, ``pyo3`` (technically ``pyo3-build-config``)
will attempt to locate a ``libpython`` to link against. This behavior is
documented at https://pyo3.rs/v0.15.0/building_and_distribution.html.

Generally speaking, all the caveats documented by ``pyo3`` apply to
``pyembed`` as well, since this project is a glorified, value-adding
wrapper around ``pyo3``.

The short version of the PyO3 documentation is as follows:

* By default the build script will look for an executable ``python`` on
  ``PATH`` and attempt to derive its build configuration from it.
* You can point it at a specific Python executable by setting the
  ``PYO3_PYTHON`` environment variable.
* For more advanced use cases (including cross-compiling), you can
  create a custom config file to configure the ``pyo3-build-config``
  crate and point to it via the ``PYO3_CONFIG_FILE`` environment
  variable.

Generally speaking, if you are able to build the ``pyo3`` crate in
isolation, you should be able to build the ``pyembed`` crate. To
customize how the ``pyembed`` crate links against Python, use
``pyo3``'s mechanisms for doing that.
