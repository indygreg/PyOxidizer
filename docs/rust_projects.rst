.. _rust_projects:

=============
Rust Projects
=============

PyOxidizer uses Rust projects to build binaries embedding Python.

If you just have a standalone configuration file (such as when running
``pyoxidizer init-config-file``), a temporary Rust project will be
created as part of building binaries and the existence of Rust should
be largely invisible (except for the output from building the Rust project).

If you use ``pyoxidizer init-rust-project`` to initialize a
``PyOxidizer`` application, the Rust project exists side-by-side with
the ``PyOxidizer`` configuration file and can be modified like
any other Rust project.

Either way, the ``PyOxidizer`` configuration file works alongside Rust
to build binaries.

.. _rust_project_layout:

Layout
======

Generated Rust projects all have a similar layout::

   $ find pyapp -type f | grep -v .git
   Cargo.toml
   src/main.rs
   pyembed/Cargo.toml
   pyembed/build.rs
   pyembed/src/config.rs
   pyembed/src/data.rs
   pyembed/src/importer.rs
   pyembed/src/lib.rs
   pyembed/src/pyalloc.rs
   pyembed/src/pyinterp.rs
   pyembed/src/pystr.rs

Main Application Project
========================

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
to ``exit()``. Succinctly, we instantiate and run an embedded Python
interpreter. That's our executable.

The ``pyembed`` Package
=======================

The bulk of the files in our new project are in the ``pyembed`` directory.
This directory defines a Rust project whose job it is to build and manage
an embedded Python interpreter. This project behaves like any other Rust
library project: there's a ``Cargo.toml``, a ``src/lib.rs`` defining the
main library define, and a pile of other ``.rs`` files implementing the
library functionality. The only functionality you will likely be concerned
about are the ``PythonConfig`` and ``MainPythonInterpreter`` structs. These
types define how the embedded Python interpreter is configured and executed.
If you want to learn more about this crate and how it works, run ``cargo doc``
and read :ref:`pyembed`.

There are a few special properties about the ``pyembed`` package worth
calling out.

First, the package is a copy of files from the PyOxidizer project. Typically,
one could reference a crate published on a package repository like
https://crates.io/ and we wouldn't need to have local files. However,
``pyembed`` is currently relying on modifications to some other published
crates (we plan to upstream all changes eventually). This means we can't
publish ``pyembed`` on ``crates.io``. So we need to vendor a copy next to
your project. Sorry about the (temporary) inconvenience!

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
