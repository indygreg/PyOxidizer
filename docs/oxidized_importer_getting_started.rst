.. _oxidized_importer_getting_started:

===============
Getting Started
===============

Requirements
============

``oxidized_importer`` requires CPython 3.8 or newer. This is because it
relies on modern C and Python standard library APIs only available in that
version.

Building ``oxidized_importer`` from source requires a working Rust toolchain
for the target platform.

Installing from PyPI
====================

``oxidized_importer`` is
`available <https://pypi.org/project/oxidized_importer/>`_ on PyPI. This
means that installing is as simple as::

   $ pip3 install oxidized_importer

Compiling from Source
=====================

To build from source, obtain a clone of PyOxidizer's Git repository and
run the ``setup.py`` script or use ``pip`` to build the Python project in
the root of the repository. e.g.::

   $ python3.8 setup.py build_ext -i
   $ python3.8 setup.py install

   $ pip3.8 install .
   $ pip3.8 wheel .

The ``setup.py`` is pretty minimal and is a thin wrapper around ``cargo build``
for the underlying Rust project. If you want to build using Rust's standard
toolchain, do something like the following::

   $ cd oxidized-importer
   $ cargo build --release

If you don't have a Python 3.8 ``python3`` executable in your ``PATH``, you
will need to tell the Rust build system which ``python3`` executable to use to
help derive the build configuration for the Python extension::

   $ PYTHON_SYS_EXECUTABLE=/path/to/python3.8 cargo build

Using
=====

To use ``oxidized_importer``, simply import the module:

.. code-block:: python

   import oxidized_importer

To register a custom importer with Python, do something like the following:

.. code-block:: python

   import sys

   import oxidized_importer

   finder = oxidized_importer.OxidizedFinder()

   # You want to register the finder first so it has the highest priority.
   sys.meta_path.insert(0, finder)

To get performance benefits of loading modules and resources from memory,
you'll need to index resources with the ``OxidizedFinder``, serialize that
data out, then load that data into a new ``OxidizedFinder`` instance. See
:ref:`oxidized_importer_freezing` for more detailed examples.
