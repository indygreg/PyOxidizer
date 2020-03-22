.. _managing_projects:

====================================
The ``pyoxidizer`` Command Line Tool
====================================

The ``pyoxidizer`` command line tool is a frontend to the various
functionality of ``PyOxidizer``. See :ref:`components` for more
on the various components of ``PyOxidizer``.

Creating New Projects with ``init-config-file``
===============================================

The ``pyoxidizer init-config-file`` command will create a new
``pyoxidizer.bzl`` configuration file in the target directory::

   $ pyoxidizer init-config-file pyapp

This should have printed out details on what happened and what to do next.

Creating New Rust Projects with ``init-rust-project``
=====================================================

The ``pyoxidizer init-rust-project`` command creates a minimal
Rust project configured to build an application that runs an
embedded Python interpreter from a configuration defined in a
``pyoxidizer.bzl`` configuration file. Run it by specifying the
directory to contain the new project::

   $ pyoxidizer init-rust-project pyapp

This should have printed out details on what happened and what to do next.

The explicit creation of Rust projects to use ``PyOxidizer`` is not
required. If your produced binaries only need to perform actions
configurable via ``PyOxidizer`` configuration files (like running
some Python code), an explicit Rust project isn't required, as
``PyOxidizer`` can auto-generate a temporary Rust project at build time.

But if you want to supplement the behavior of the binaries built
with Rust, an explicit and persisted Rust project can facilitate that.
For example, you may want to run custom Rust code before, during, and
after a Python interpreter runs in the process.

See :ref:`rust_projects` for more on the composition of Rust projects.

Adding PyOxidizer to an Existing Project with ``add``
=====================================================

Do you have an existing Rust project that you want to add an embedded
Python interpreter to? PyOxidizer can help with that too! The
``pyoxidizer add`` command can be used to add an embedded Python
interpreter to an existing Rust project. Simply give the directory
to a project containing a ``Cargo.toml`` file::

   $ cargo init myrustapp
     Created binary (application) package
   $ pyoxidizer add myrustapp

This will add required files and make required modifications to add
an embedded Python interpreter to the target project.

.. important::

   It is highly recommended to have the destination project under version
   control so you can see what changes are made by ``pyoxidizer add`` and
   so you can undo any unwanted changes.

.. danger::

   This command isn't very well tested. And results have been known to be
   wrong. If it doesn't *just work*, you may want to run ``pyoxidizer init``
   and incorporate relevant files into your project manually. Sorry for
   the inconvenience.

Building PyObject Projects with ``build``
=========================================

The ``pyoxidizer build`` command is probably the most important and used
``pyoxidizer`` command. This command evaluates a ``pyoxidizer.bzl``
configuration file by resolving *targets* in it.

By default, the default *target* in the configuration file is resolved.
However, callers can specify a list of explicit *targets* to resolve.
e.g.::

   # Resolve the default target.
   $ pyoxidizer build

   # Resolve the "exe" and "install" targets, in that order.
   $ pyoxidizer build exe install

``PyOxidizer`` configuration files are effectively defining a build
system, hence the name *build* for the command to resolve *targets*
within.

Running the Result of Building with ``run``
===========================================

Target functions in ``PyOxidizer`` configuration files return objects
that may be *runnable*. For example, a
:ref:`PythonExecutable <config_python_executable>` returned by a target
function that defines a Python executable binary can be *run* by
executing a new process.

The ``pyoxidizer run`` command is used to attempt to *run* an object
returned by a build target. It is effectively ``pyoxidizer build`` followed
by *running* the returned object. e.g.::

   # Run the default target.
   $ pyoxidizer run

   # Run the "install" target.
   $ pyoxidizer run --target install

Analyzing Produced Binaries with ``analyze``
============================================

The ``pyoxidizer analyze`` command is a generic command for analyzing the
contents of executables and libraries. While it is generic, its output is
specifically tailored for ``PyOxidizer``.

Run the command with the path to an executable. For example::

   $ pyoxidizer analyze build/apps/myapp/x86_64-unknown-linux-gnu/debug/myapp

Behavior is dependent on the format of the file being analyzed. But the
general theme is that the command attempts to identify the run-time
requirements for that binary. For example, for ELF binaries it will
list all shared library dependencies and analyze ``glibc`` symbol
versions and print out which Linux distributions it thinks the binary
is compatible with.

.. note::

   ``pyoxidizer analyze`` is not yet implemented for all executable
   file types that ``PyOxidizer`` supports.

Inspecting Python Distributions
===============================

``PyOxidizer`` uses special pre-built Python distributions to build
binaries containing Python.

These Python distributions are zstandard compressed tar files. Zstandard
is a modern compression format that is really, really, really good.
(PyOxidizer's maintainer also maintains
`Python bindings to zstandard <https://github.com/indygreg/python-zstandard>`_
and has
`written about the benefits of zstandard <https://gregoryszorc.com/blog/2017/03/07/better-compression-with-zstandard/>`_
on his blog. You should read that blog post so you are enlightened on
how amazing zstandard is.) But because zstandard is relatively new, not
all systems have utilities for decompressing that format yet. So, the
``pyoxidizer python-distribution-extract`` command can be used to extract
the zstandard compressed tar archive to a local filesystem path.

Python distributions contain software governed by a number of licenses.
This of course has implications for application distribution. See
:ref:`licensing_considerations` for more.

The ``pyoxidizer python-distribution-licenses`` command can be used to
inspect a Python distribution archive for information about its licenses.
The command will print information about the licensing of the Python
distribution itself along with a per-extension breakdown of which
libraries are used by which extensions and which licenses apply to what.
This command can be super useful to audit for license usage and only allow
extensions with licenses that you are legally comfortable with.

For example, the entry for the ``readline`` extension shows that the
extension links against the ``ncurses`` and ``readline`` libraries, which
are governed by the X11, and GPL-3.0 licenses::

   readline
   --------

   Dependency: ncurses
   Link Type: library

   Dependency: readline
   Link Type: library

   Licenses: GPL-3.0, X11
   License Info: https://spdx.org/licenses/GPL-3.0.html
   License Info: https://spdx.org/licenses/X11.html

.. note::

   The license annotations in Python distributions are best effort and
   can be wrong. They do not constitute a legal promise. Paranoid
   individuals may want to double check the license annotations by
   verifying with source code distributions, for example.
