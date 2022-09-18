==================
PyOxidizer Project
==================

Welcome to the unified documentation of the
`PyOxidizer Project <https://github.com/indygreg/PyOxidizer>`_, a collection
of libraries and tools attempting to improve ergonomics around packaging
and distributing [Python] applications.

The official home of the project is https://github.com/indygreg/PyOxidizer.
Official documentation lives on gregoryszorc.com
(`unreleased/latest commit <https://gregoryszorc.com/docs/pyoxidizer/main/>`_,
`last release <https://gregoryszorc.com/docs/pyoxidizer/stable/>`_).

The `pyoxidizer-users <https://groups.google.com/forum/#!forum/pyoxidizer-users>`_
mailing list is a forum for users to discuss all things PyOxidizer.

The creator and maintainer of ``PyOxidizer`` is
`Gregory Szorc <https://gregoryszorc.com/>`_.

Multiple Tools Under One Roof
=============================

The PyOxidizer Project is comprised of discrete pieces of software developed
in the same repository. Major pieces of user-facing software have their own
documentation, each described in the following sections.

oxidized_importer
-----------------

A Python extension module [implemented in Rust] providing a highly performant
alternate module and resource importing mechanism. ``oxidzed_importer`` can
be used to import Python modules and resources from memory, enabling Python
applications to be single file executables.

``oxidized_importer`` is usable as a standalone Python package and can
be installed `from PyPI <https://pypi.org/project/oxidized-importer/>`_.

.. toctree::
   :maxdepth: 2

   oxidized_importer

pyembed
-------

A Rust library crate to control embedded Python interpreters in Rust
applications. The ``pyembed`` crate enhances the functionality of embedded
Python interpreters by implementing additional features such as integration
with :ref:`oxidized_importer <oxidized_importer>`, easy configuration of
alternate memory allocators, automatic terminfo database resolution, and
more.

``pyembed`` is usable as a standalone Rust crate and can be used by any
Rust project embedding Python to abstract over some of the complexities
with embedding a Python interpreter.

.. toctree::
   :maxdepth: 2

   pyembed

PyOxidizer
----------

PyOxidizer is a [Rust] application for streamlining the creation of
distributable Python applications.

PyOxidizer is often used to generate binaries embedding a Python
interpreter and a custom Python application. However, its configuration
files support additional functionality, such as the ability to produce
Windows MSI installers, macOS application bundles, and more.

PyOxidizer is primarily made available as the ``pyoxidizer`` command line
tool. However, it is also usable as a Rust library crate.

.. toctree::
   :maxdepth: 2

   pyoxidizer

PyOxy
-----

PyOxy is an application providing an alternative Python runner. Think of
it as an alternative implementation and re-imagination of the ubiquitous
``python`` command.

PyOxy enables access to some of the technology built for ``pyoxidizer``
(notably :ref:`oxidized_importer <oxidized_importer>` and
:ref:`pyembed <pyembed>`) without having to use ``pyoxidizer``.

PyOxy is distributed as a standalone application.

.. toctree::
   :maxdepth: 2

   pyoxy

Tugger
------

Tugger is an umbrella project for implementing generic application packaging
and distribution functionality. It is comprised as several Rust crates,
each providing domain-specific functionality including:

* Debian packaging formats
* Software licensing
* Snapcraft packaging
* Apple code signing
* Rust toolchain installation
* Windows installer generation
* And much more

Tugger defines Starlark primitives for scripting common application packaging
and distribution actions.

Tugger is used by PyOxidizer for performing functionality that isn't
specific to Python.

There are aspirations to make Tugger a standalone tool someday. But for now,
it is only available as a series of Rust crates.

.. toctree::
   :maxdepth: 2

   tugger
