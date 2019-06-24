.. _managing_projects:

=====================================
Managing Projects with ``pyoxidizer``
=====================================

The ``pyoxidizer`` command line tool is used to manage the integration
of ``PyOxidizer`` within a Rust project. See :ref:`components` for more
on the various components of ``PyOxidizer``.

High-Level Project Lifecycle and Pipeline
=========================================

``PyOxidizer`` projects conceptually progress through a development
pipeline. This pipeline consists of the following phases:

1. Creation
2. Python Building
3. Application Building
4. Application Assembly
5. Validation (manual)
6. Distribution (not yet implemented)

In ``Creation``, a new project is created.

In ``Python Building``, the Python components of the project are
derived. This includes fetching any Python package dependencies.

In ``Application Building``, the larger [Rust] application is built.
this usually entails producing an executable containing an embedded
Python interpreter along with any embedded python resource data.

In ``Application Assembly``, the built [Rust] application is assembled
with other packaging pieces. These extra pieces could include Python
modules not embedded in the [Rust] binary.

In ``Validation``, the assembled application is validated, tested, etc.

In ``Distribution``, distributable versions of the assembled application
are produced. This includes installable packages, etc.

Typically, ``Python Building``, ``Application Building``, and
``Application Assembly`` are performed as a single logical step
(often via ``pyoxidizer build``). But ``PyOxidizer`` supports performing
each action in isolation in order to facilitate more flexible development
patterns.

Creating New Projects with ``init``
===================================

The ``pyoxidizer init`` command will create a new [Rust] project which supports
embedding Python. Invoke it with the directory you want to create your new
project in::

   $ pyoxidizer init pyapp

This should have printed out details on what happened and what to do next.
If you actually ran this in a terminal, hopefully you don't need to continue
following the directions here as the printed instructions are sufficient!

Before we move on, let's explore what new projects look like.

.. _new_project_layout:

New Project Layout
==================

``pyoxidizer init`` essentially does two things:

1. Creates a new Rust executable project by running ``cargo init``.
2. Adds PyOxidizer files to that project.

If we run ``pyoxidizer init pyapp``, let's explore our newly-created ``pyapp``
project::

   $ find pyapp -type f | grep -v .git
   pyapp/Cargo.toml
   pyapp/src/main.rs
   pyapp/pyoxidizer.toml
   pyapp/pyembed/src/config.rs
   pyapp/pyembed/src/importer.rs
   pyapp/pyembed/src/data.rs
   pyapp/pyembed/src/lib.rs
   pyapp/pyembed/src/pyinterp.rs
   pyapp/pyembed/src/pyalloc.rs
   pyapp/pyembed/src/pystr.rs
   pyapp/pyembed/build.rs
   pyapp/pyembed/Cargo.toml

The Main Project
----------------

The ``Cargo.toml`` file is the configuration file for the Rust project.
Read more in
`the official Cargo documentation <https://doc.rust-lang.org/cargo/reference/manifest.html>`_.
The magic lines in this file to enable PyOxidizer are the following::

   [dependencies]
   pyembed = { path = "pyembed" }

These lines declare a dependency on the ``pyembed`` package in the directory
``pyembed``. ``Cargo.toml`` is overall pretty straightforward.

Next let's look at ``pyapp/src/main.rs``. If you aren't familiar with Rust
projects, the ``src/main.rs`` file is the default location for the source
file implementing an executable. If we open that file, we see a
``fn main() {`` line, which declares the *main* function for our executable.
The file is relatively straightforward. We import some symbols from the
``pyembed`` crate. We then construct a config object, use that to construct
a Python interpreter, then we run the interpreter and pass its exit code
to ``exit()``.

The ``pyembed`` Package
-----------------------

The bulk of the files in our new project are in the ``pyembed`` directory.
This directory defines a Rust project whose job it is to build and manage
an embedded Python interpreter. This project behaves like any other Rust
library project: there's a ``Cargo.toml``, a ``src/lib.rs`` defining the
main library define, and a pile of other ``.rs`` files implementing the
library functionality. The only functionality you will likely be concerned
about are the ``PythonConfig`` and ``MainPythonInterpreter`` structs. These
types define how the embedded Python interpreter is configured and executed.
If you want to learn more about this crate and how it works, run ``cargo doc``.

There are a few special properties about the ``pyembed`` package worth
calling out.

First, the package is a copy of files from the PyOxidizer project. Typically,
one could reference a crate published on a package repository like
https://crates.io/ and we wouldn't need to have local files. However,
``pyembed`` is currently relying on modifications to some other published
crates (we plan to upstream all changes eventually). This means we can't
publish ``pyembed`` on crates.io. So we need to vendor a copy next to your
project. Sorry about the inconvenience!

Speaking of modification to the published crates, the ``pyembed``'s
``Cargo.toml`` enumerates those crates. If ``pyoxidizer`` was run from
an installed executable, these modified crates will be obtained from
PyOxidizer's canonical Git repository. If ``pyoxidizer`` was run out of
the PyOxidizer source repository, these modified crates will be obtained
from the local filesystem path to that repository. **You may want to
consider making copies of these crates and/or vendoring them next to your
project if you aren't comfortable fetching dependencies from the local
filesystem or a Git repository.**

Another property about ``pyembed`` worth mentioning is its ``build.rs`` build
script. This program runs as part of building the library. As you can
see from the source, this program attempts to locate a ``pyoxidizer``
executable and then calls ``pyoxidizer run-build-script``. ``pyoxidizer``
thus provides the bulk of the build script functionality. This is slightly
unorthodox. But it enables you to build applications without building all
of PyOxidizer. And since PyOxidizer has a few hundred package dependencies,
this saves quite a bit of time!

The ``pyoxidizer.toml`` Configuration File
------------------------------------------

The final file in our newly created project is ``pyoxidizer.toml``. **It is
the most important file in the project.**

The ``pyoxidizer.toml`` file configures how the embedded Python interpreter
is built. This includes choosing which modules to package. It also configures
the default run-time settings for the interpreter, including which code to
run.

See :ref:`config_files` for comprehensive documentation of ``pyoxidizer.toml``
files and their semantics.

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
an embedded Python interpreter to the target project. Most of the
modifications are in the form of a new ``pyembed`` crate.

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
``pyoxidizer`` command. This command does the following:

1. Processes the ``pyoxidizer.toml`` configuration file and derives Python
   artifacts to incorporate in a larger binary. (The ``Python Building``
   phase of the pipeline described at the top of this document.)
2. Invokes ``cargo build`` to build the associated Rust project.
   (The ``Application Building`` phase.)
3. Performs any post-build actions to assemble extra resources alongside
   the ``cargo``-built binary. (The ``Application Assembly`` phase.)

In short, ``pyoxidizer build`` attempts to build your application as you
have configured it.

``Application Assembly`` is performed into a ``build/apps/<app>`` directory
under the project root. If your project name is ``myapp``, the application
will be assembled to a ``build/apps/myapp`` directory. The full path to the
executable will be ``build/apps/myapp/myapp`` (on Linux and macOS) or
``build/apps/myapp/myapp.exe`` (on Windows).

It's worth noting that the ergonomics of ``pyoxidizer build`` are superior to
``cargo build``. With ``pyoxidizer build``, the tool prints information about
Python-specific activity as it is occurring. While it is possible to build
applications with ``cargo build`` to achieve the same effect, doing so will
defer Python build steps until later in the build and will hide that activity
from output. This behavior isn't optimal for people whose primary goal is to
package Python applications.

Running Applications with ``run``
=================================

Once you have produced an application with ``pyoxidizer build``, you can run
it with ``pyoxidizer run``. For example::

   $ pyoxidizer run -- foo bar'

This command will build your application (if needed) then invoke it with the
arguments specified.

This command is provided for convenience, as it is certainly possible to
run executables directly from their build location.

Analyzing Produced Binaries with ``analyze``
============================================

The ``pyoxidizer analyze`` command is a generic command for analyzing the
contents of executables and libraries. While it is generic, its output is
specifically tailored for ``PyOxidizer``.

Run the command with the path to an executable. For example::

   $ pyoxidizer analyze build/apps/myapp/myapp

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

The ``Python Building`` phase of the lifecycle entails downloading special
pre-built Python distributions and then linking them into a larger binary.
You can find the location of these distributions in your project's
``pyoxidizer.toml`` configuration file.

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
